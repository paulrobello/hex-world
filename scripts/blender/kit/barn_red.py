"""barn_red: the farm's red board-and-batten timber barn (P3 farm tile). Owner-approved references:
assets-source/refs/farm (ref-1, ref-2). The owner's A17 answer (approval.json) wins over the refs:
a pitched roof (ref-2 draws a gambrel), red vertical-board timber for contrast with the cottages,
large double doors and a dark slate roof. Only the barn building is modelled; the field, fence,
bales and yard are separate pieces. Scripted route, self-contained (no `_house` import: that helper
is owned by a sibling piece in this batch, and the barn's ridge runs on the other axis).

Frame: Blender (x, y, z) exports as runtime (x, z, -y). The pivot is the footprint centre at ground
level (z = 0). The ridge runs along x and the front gable, with the double doors and the hayloft
door, faces +x (yaw 0, as the houses' doors do). Walls, doors and plinth run from DROP (below the
meadow cap's deepest relief, -0.25 m) so the barn never floats over a cap dip.

Scale: footprint 10 m long (x) by 7 m wide (y), the roof surface 4.2 m up at the wall line, a 45°
slate roof to a 7.7 m ridge. Beside house_cottage (4.4 x 6.4 m, 47°, ridge 4.96 m) and
house_twostorey (5.0 x 7.2 m, eaves 5.0 m, ridge 7.1 m) it reads as the larger farm building, at
real-metre scale with the diorama's slightly exaggerated roof.

Budget class: a prop (PRD §6): LOD0 <= 1,500 tris, LOD1 <= 50 %, LOD2 <= 15 %, KTX2 class `piece`.

Look (the house approach, project.md "Budgets"): every isotropic surface samples the world-space
`plaster` set tinted by its manifest colour (barn red boards, darker red door leaves, dark slate
roof, grey stone plinth). Directional detail is geometry: board battens every 0.5 m, slate courses,
white corner boards, door frames and braces, bargeboards and a white fascia. Trim and glass are
colour-only slots. The only unique map is `masks` (UV0 atlas over all three LODs, each LOD baked
onto its own islands): R snow 1 (0 on glass), G porosity per surface, B night glow on the glass, A
baked AO. Faces nobody can see are never built (no plinth top under the floor, no soffit over the
interior, no wall inside the roof slab), so the atlas spends its texels on visible surfaces.

LODs are rebuilt from the same parameters at lower detail, never decimated: LOD1 drops the battens,
half the slate courses, the door braces and the window bars; LOD2 is the wall shell, a plain roof,
flat door leaves and the glass."""
import math
import os
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import bmesh  # noqa: E402
import bpy  # noqa: E402
import numpy as np  # noqa: E402
from mathutils import Vector  # noqa: E402

from lib import bake, budgets, collider, export, geom, materials, scene, uv  # noqa: E402

P = "barn_red"
DROP = -0.35
MASK_SIZE = 1024
SAMPLES = 64
AO_DIST = 0.6
# Linear albedo of the plaster set (mean of plaster.albedo.png), so Blender renders of tinted
# tileable slots show the runtime colour: runtime = manifest colour x set albedo.
PLASTER_LIN = 0.66

# Body.
HL, HW = 5.0, 3.5  # half length (x, along the ridge) and half width (y)
EAVE = 4.2  # roof surface height at the wall line
PITCH = math.radians(45.0)
TAN, COS, SIN = math.tan(PITCH), math.cos(PITCH), math.sin(PITCH)
OVERHANG, GABLE_OVERHANG = 0.35, 0.3
COURSES = (18, 9, 0)  # slate courses per slope, per LOD (0.30 m at LOD0, as on the houses)
KICK, ROOF_THICK = 0.03, 0.15
PLINTH_TOP, PLINTH_OUT = 0.35, 0.05
# Board battens: 9 cm wide, 2.5 cm proud, every 0.5 m, laid at half steps so the openings (centred on
# whole steps) fall between them.
STEP, BW, BP = 0.5, 0.09, 0.025
# Corner boards: 18 cm, 3.5 cm proud of both faces.
CB, CO = 0.18, 0.035
# Double doors on the front gable: two 1.5 m leaves, 3.4 m high, in a 16 cm white frame.
DOOR_HALF, DOOR_H, DOOR_TRIM = 1.5, 3.4, 0.16
# Hayloft door above them.
HAY_HALF, HAY_Z0, HAY_Z1 = 0.55, 4.55, 5.85
FRAME = 0.1

# Slots: (manifest colour, roughness, tileable, hooks (snow, porosity, night)). Tileable slots show
# manifest colour x plaster albedo; colour-only slots show the manifest colour.
SLOTS = {
    "walls": ((0.45, 0.07, 0.045), 0.9, True, (1.0, 0.55, 0.0)),
    "door": ((0.32, 0.055, 0.04), 0.85, True, (1.0, 0.55, 0.0)),
    "roof": ((0.105, 0.11, 0.125), 0.7, True, (1.0, 0.2, 0.0)),
    "stone": ((0.42, 0.39, 0.34), 0.9, True, (1.0, 0.4, 0.0)),
    "trim": ((0.70, 0.69, 0.65), 0.6, False, (1.0, 0.4, 0.0)),
    "glass": ((0.20, 0.16, 0.10), 0.2, False, (0.0, 0.05, 1.0)),
}
NAMES = [f"{P}_{s}" for s in SLOTS]

# Windows: (wall, u, sill z, width, height). Wall u runs along +y on the front gable, -y on the back
# gable, +x on the left long wall (-y) and -x on the right (+y).
WINDOWS = [("left", -2.5, 1.7, 0.8, 0.9), ("left", 2.5, 1.7, 0.8, 0.9),
           ("right", -2.5, 1.7, 0.8, 0.9), ("right", 2.5, 1.7, 0.8, 0.9),
           ("back", 0.0, 4.6, 0.9, 0.9)]

Z = Vector((0.0, 0.0, 1.0))


def under(y):
    """Height of the roof slab's underside (soffit) above horizontal distance |y| from the ridge."""
    return EAVE + (HW - abs(y)) * TAN - ROOF_THICK / COS


WALL_TOP = under(HW)


class Mesh:
    """bmesh accumulator with per-face slot indices."""

    def __init__(self):
        self.bm = bmesh.new()
        self.idx = {s: i for i, s in enumerate(SLOTS)}

    def face(self, pts, slot, expect):
        f = self.bm.faces.new([self.bm.verts.new(p) for p in pts])
        f.material_index = self.idx[slot]
        f.normal_update()
        if f.normal.dot(expect) < 0:
            f.normal_flip()
        return f

    def obj(self, name):
        bmesh.ops.remove_doubles(self.bm, verts=self.bm.verts, dist=1e-5)
        bmesh.ops.triangulate(self.bm, faces=self.bm.faces)
        mesh = bpy.data.meshes.new(name)
        self.bm.to_mesh(mesh)
        self.bm.free()
        o = scene.link(bpy.data.objects.new(name, mesh))
        for n in NAMES:
            o.data.materials.append(bpy.data.materials[n])
        return o


class Wall:
    """A wall face with outward normal n and horizontal tangent t = Z x n; p(u, z, out) is the point
    u along t, at height z, `out` proud of the face. top(u, out) is the soffit above that point."""

    def __init__(self, n, origin, gable):
        self.n = Vector(n)
        self.t = Z.cross(self.n)
        self.o = Vector(origin)
        self.gable = gable

    def p(self, u, z, out=0.0):
        return self.o + self.t * u + Z * z + self.n * out

    def top(self, u, out):
        return under(u) if self.gable else under(HW + out)

    def slab(self, m, u0, u1, z0, z1, o0, o1, slot, faces="flrt"):
        """A box on the wall over u0..u1, z0..z1, o0..o1 proud; faces f(ront), l (u0), r (u1),
        t(op), b(ottom). The back face is never built. z1 may be a pair (top at u0, top at u1) for
        a top that follows a soffit."""
        p = self.p
        za, zb = z1 if isinstance(z1, tuple) else (z1, z1)
        if "f" in faces:
            m.face([p(u0, z0, o1), p(u1, z0, o1), p(u1, zb, o1), p(u0, za, o1)], slot, self.n)
        if "l" in faces:
            m.face([p(u0, z0, o0), p(u0, z0, o1), p(u0, za, o1), p(u0, za, o0)], slot, -self.t)
        if "r" in faces:
            m.face([p(u1, z0, o0), p(u1, zb, o0), p(u1, zb, o1), p(u1, z0, o1)], slot, self.t)
        if "t" in faces:
            m.face([p(u0, za, o0), p(u0, za, o1), p(u1, zb, o1), p(u1, zb, o0)], slot, Z)
        if "b" in faces:
            m.face([p(u0, z0, o0), p(u1, z0, o0), p(u1, z0, o1), p(u0, z0, o1)], slot, -Z)

    def strip(self, m, a, b, width, o0, o1, slot):
        """A trim strip from wall point a to b (u, z), any angle: its front and two long sides."""
        a, b = Vector(a), Vector(b)
        d = (b - a).normalized()
        s = Vector((-d.y, d.x)) * (width / 2)
        q = [a - s, b - s, b + s, a + s]
        m.face([self.p(c.x, c.y, o1) for c in q], slot, self.n)
        for c0, c1, side in ((q[0], q[1], -s), (q[2], q[3], s)):
            out = self.t * side.x + Z * side.y
            m.face([self.p(c0.x, c0.y, o0), self.p(c1.x, c1.y, o0), self.p(c1.x, c1.y, o1),
                    self.p(c0.x, c0.y, o1)], slot, out)

    def frame(self, m, u, z0, w, h, detail, out):
        """A white picture frame ring round an opening (u centre, z0 bottom): its front ring `out`
        proud, the outer sides (detail 0-1) and the inner reveal down to 1.5 cm (detail 0); detail 2
        is the front ring alone."""
        f = FRAME
        u0, u1, z1 = u - w / 2, u + w / 2, z0 + h
        outer = [(u0 - f, z0 - f), (u1 + f, z0 - f), (u1 + f, z1 + f), (u0 - f, z1 + f)]
        inner = [(u0, z0), (u1, z0), (u1, z1), (u0, z1)]
        for i in range(4):
            j = (i + 1) % 4
            (ou0, oz0), (ou1, oz1) = outer[i], outer[j]
            (iu0, iz0), (iu1, iz1) = inner[i], inner[j]
            p = self.p
            m.face([p(ou0, oz0, out), p(ou1, oz1, out), p(iu1, iz1, out), p(iu0, iz0, out)], "trim", self.n)
            side = (self.t * ((ou0 + ou1) / 2 - u) + Z * ((oz0 + oz1) / 2 - (z0 + z1) / 2)).normalized()
            if detail < 2:
                m.face([p(ou0, oz0, 0), p(ou1, oz1, 0), p(ou1, oz1, out), p(ou0, oz0, out)], "trim", side)
            if detail == 0:
                m.face([p(iu0, iz0, out), p(iu1, iz1, out), p(iu1, iz1, 0.015), p(iu0, iz0, 0.015)], "trim", -side)


def window(m, w, u, z0, width, height, detail):
    """Glass 1.5 cm proud in a white frame; four-pane cross bars at LOD0."""
    u0, u1, z1 = u - width / 2, u + width / 2, z0 + height
    m.face([w.p(u0, z0, 0.015), w.p(u1, z0, 0.015), w.p(u1, z1, 0.015), w.p(u0, z1, 0.015)], "glass", w.n)
    if detail < 2:
        w.frame(m, u, z0, width, height, 2 * detail, 0.06)
    if detail == 0:
        w.strip(m, (u, z0), (u, z1), 0.05, 0.015, 0.04, "trim")
        w.strip(m, (u0, (z0 + z1) / 2), (u1, (z0 + z1) / 2), 0.05, 0.015, 0.036, "trim")


def double_doors(m, w, detail):
    """Two board leaves in a white frame (jambs and header), each with a white border and a
    diagonal brace rising from its hinge side (LOD0)."""
    if detail == 2:
        for u0, u1 in ((-DOOR_HALF, 0.0), (0.0, DOOR_HALF)):
            m.face([w.p(u0, DROP, 0.03), w.p(u1, DROP, 0.03), w.p(u1, DOOR_H, 0.03), w.p(u0, DOOR_H, 0.03)], "door", w.n)
        return
    # Leaves: front and the sides at the centre slit (the outer sides and top sit under the frame).
    for u0, u1, faces in ((-DOOR_HALF, -0.01, "fr"), (0.01, DOOR_HALF, "fl")):
        w.slab(m, u0, u1, DROP, DOOR_H, 0.0, 0.08, "door", faces)
    for u0, u1 in ((-DOOR_HALF - DOOR_TRIM, -DOOR_HALF), (DOOR_HALF, DOOR_HALF + DOOR_TRIM)):
        w.slab(m, u0, u1, DROP, DOOR_H, 0.0, 0.11, "trim", "flr")
    w.slab(m, -DOOR_HALF - DOOR_TRIM, DOOR_HALF + DOOR_TRIM, DOOR_H, DOOR_H + DOOR_TRIM, 0.0, 0.11, "trim", "flrtb")
    if detail == 0:
        # Per leaf: a white border (stiles, then rails between them) and a brace from the bottom
        # hinge corner to the top latch corner, a little less proud so its ends tuck under the border.
        b, z0, z1 = 0.14, 0.05, DOOR_H - 0.02
        for hinge, latch in ((-DOOR_HALF, -0.01), (DOOR_HALF, 0.01)):
            s = 1 if latch > hinge else -1
            ho, li = hinge + s * (b / 2 + 0.02), latch - s * (b / 2 + 0.02)
            w.strip(m, (ho, z0), (ho, z1), b, 0.08, 0.105, "trim")
            w.strip(m, (li, z0), (li, z1), b, 0.08, 0.105, "trim")
            for zr in (z1 - b / 2, z0 + b / 2):
                w.strip(m, (ho + s * b / 2, zr), (li - s * b / 2, zr), b, 0.08, 0.105, "trim")
            w.strip(m, (ho + s * b / 2, z0 + b), (li - s * b / 2, z1 - b), 0.12, 0.08, 0.1, "trim")


def hayloft(m, w, detail):
    """The hayloft door: a board leaf 3 cm proud in a white frame, with a diagonal brace (LOD0)."""
    u0, u1 = -HAY_HALF, HAY_HALF
    m.face([w.p(u0, HAY_Z0, 0.03), w.p(u1, HAY_Z0, 0.03), w.p(u1, HAY_Z1, 0.03), w.p(u0, HAY_Z1, 0.03)], "door", w.n)
    if detail < 2:
        w.frame(m, 0.0, HAY_Z0, 2 * HAY_HALF, HAY_Z1 - HAY_Z0, 2 * detail, 0.06)
    if detail == 0:
        w.strip(m, (u0 + 0.08, HAY_Z0 + 0.08), (u1 - 0.08, HAY_Z1 - 0.08), 0.1, 0.03, 0.05, "trim")


def battens(m, w, half, openings):
    """Vertical battens over one wall at half steps, cut round the openings (u0, u1, z0, z1) and
    running up to the soffit. Cut ends abut a more proud frame, so they need no end faces."""
    us, u = [], STEP / 2
    while u + BW / 2 <= half - CB - 0.02:
        us += [-u, u]
        u += STEP
    for u in sorted(us):
        u0, u1 = u - BW / 2, u + BW / 2
        segs, z = [], PLINTH_TOP
        for a, b, z0, z1 in sorted(openings, key=lambda o: o[2]):
            if a < u1 and b > u0:
                if z0 > z:
                    segs.append((z, z0))
                z = max(z, z1)
        segs.append((z, None))
        for za, zb in segs:
            if zb is None:
                tf = (w.top(u0, BP), w.top(u1, BP))
                tb = (w.top(u0, 0.0), w.top(u1, 0.0))
            else:
                tf = tb = (zb, zb)
            if min(tf) - za < 0.15:
                continue
            p = w.p
            m.face([p(u0, za, BP), p(u1, za, BP), p(u1, tf[1], BP), p(u0, tf[0], BP)], "walls", w.n)
            m.face([p(u0, za, 0.0), p(u0, za, BP), p(u0, tf[0], BP), p(u0, tb[0], 0.0)], "walls", -w.t)
            m.face([p(u1, za, 0.0), p(u1, tb[1], 0.0), p(u1, tf[1], BP), p(u1, za, BP)], "walls", w.t)


def roof_profile(courses):
    """Closed (y, z) section of the roof, ridge on the x axis, z from the eave line: the front slope's
    slate courses (each butt `KICK` proud of the course above it) up to the ridge, mirrored for the
    back slope, over a slab ROOF_THICK deep. Returns (points, soffit indices) with the points
    counter-clockwise; the two soffit edges (back eave to under-ridge, under-ridge to front eave)
    are built separately, only where they can be seen."""
    run = HW + OVERHANG
    ridge = run * TAN
    slope = run / COS
    nrm = Vector((SIN, COS))
    up = Vector((-COS, SIN))
    eave = Vector((run, 0.0))
    front = []
    n = max(courses, 1)
    c = slope / n
    for k in range(n):
        front.append(eave + up * (k * c) + nrm * (KICK if courses else 0.0))
        if courses and k < n - 1:
            front.append(eave + up * ((k + 1) * c))
    top = Vector((0.0, ridge + (KICK if courses else 0.0)))
    front_pts = [eave - nrm * ROOF_THICK] + front
    back_pts = [Vector((-q.x, q.y)) for q in reversed(front_pts)]
    under_ridge = Vector((0.0, ridge - ROOF_THICK / COS))
    return front_pts + [top] + back_pts + [under_ridge]


def roof(m, detail):
    """The slab swept along x with its gable overhangs, slate courses, a white fascia on each eave
    edge, the visible soffit strips, and either gable end caps (LOD2) or white bargeboards on the
    rakes and a slate ridge cap (LOD0-1; the bargeboards cover the whole end cap)."""
    pts = roof_profile(COURSES[detail])
    zb = EAVE - OVERHANG * TAN
    x0, x1 = -HL - GABLE_OVERHANG, HL + GABLE_OVERHANG
    k = len(pts)
    ring0 = [m.bm.verts.new((x0, q.x, zb + q.y)) for q in pts]
    ring1 = [m.bm.verts.new((x1, q.x, zb + q.y)) for q in pts]
    fascia = {0, k - 3}  # front: eave-under -> first butt; back: last butt -> back eave-under
    for i in range(k - 2):  # skip the two soffit edges (k-2 -> k-1 -> 0)
        j = i + 1
        d = pts[j] - pts[i]
        out = Vector((0.0, d.y, -d.x))  # outward of a counter-clockwise (y, z) edge
        f = m.bm.faces.new((ring0[i], ring0[j], ring1[j], ring1[i]))
        f.material_index = m.idx["trim" if i in fascia else "roof"]
        f.normal_update()
        if f.normal.dot(out) < 0:
            f.normal_flip()
    if detail == 2:
        for ring, nx in ((ring0, -1.0), (ring1, 1.0)):
            f = m.bm.faces.new(ring)
            f.material_index = m.idx["roof"]
            f.normal_update()
            if f.normal.x * nx < 0:
                f.normal_flip()
    # Soffits (they face down): under each eave the full length, and under each gable overhang
    # between the walls.
    for s in (-1.0, 1.0):
        e = Vector((s * (HW + OVERHANG - SIN * ROOF_THICK), zb - COS * ROOF_THICK))
        wl = Vector((s * HW, WALL_TOP))
        ur = Vector((0.0, under(0.0)))
        m.face([(x0, e.x, e.y), (x1, e.x, e.y), (x1, wl.x, wl.y), (x0, wl.x, wl.y)], "roof", -Z)
        for xa, xb in ((HL, x1), (x0, -HL)):
            m.face([(xa, wl.x, wl.y), (xb, wl.x, wl.y), (xb, ur.x, ur.y), (xa, ur.x, ur.y)], "roof", -Z)
    if detail < 2:
        # Ridge cap: two slate boards 0.2 m down each side, standing clear of the top course.
        top = zb + (HW + OVERHANG) * TAN + KICK
        apex, side = top + 0.05, top - 0.2 * TAN + 0.02
        for s in (-1.0, 1.0):
            m.face([(x0, 0.0, apex), (x1, 0.0, apex), (x1, s * 0.2, side), (x0, s * 0.2, side)], "roof",
                   Vector((0.0, s * SIN, COS)))
        # Bargeboards on the four rakes: 4 cm thick against the end caps, from 6 cm below the soffit
        # to the course tops, running from the eave to the ridge.
        run = HW + OVERHANG
        for xs in (-1.0, 1.0):
            for s in (-1.0, 1.0):
                nrm = Vector((0.0, s * SIN, COS))
                up = Vector((0.0, -s * COS, SIN))
                ex = Vector((0.0, s * run, zb))
                lo, hi = -ROOF_THICK - 0.06, KICK + 0.01
                a, b = ex + nrm * lo, ex + nrm * hi
                ln = run / COS
                xa, xb = xs * (HL + GABLE_OVERHANG), xs * (HL + GABLE_OVERHANG + 0.04)
                X = Vector((xs, 0.0, 0.0))

                def pt(q, x):
                    return Vector((x, q.y, q.z))
                quad = [a, a + up * ln, b + up * ln, b]
                m.face([pt(q, xb) for q in quad], "trim", X)
                m.face([pt(b, xa), pt(b + up * ln, xa), pt(b + up * ln, xb), pt(b, xb)], "trim", nrm)
                m.face([pt(a, xa), pt(a, xb), pt(a + up * ln, xb), pt(a + up * ln, xa)], "trim", -nrm)
                m.face([pt(a, xa), pt(b, xa), pt(b, xb), pt(a, xb)], "trim", -up)


def plinth(m):
    """The stone plinth: its four outer faces and the 5 cm ledge round the wall foot (no top under
    the floor, no bottom)."""
    ox, oy = HL + PLINTH_OUT, HW + PLINTH_OUT
    outer = [(ox, -oy), (ox, oy), (-ox, oy), (-ox, -oy)]
    inner = [(HL, -HW), (HL, HW), (-HL, HW), (-HL, -HW)]
    normals = [Vector((1, 0, 0)), Vector((0, 1, 0)), Vector((-1, 0, 0)), Vector((0, -1, 0))]
    for i in range(4):
        j = (i + 1) % 4
        (ax, ay), (bx, by) = outer[i], outer[j]
        m.face([(ax, ay, DROP), (bx, by, DROP), (bx, by, PLINTH_TOP), (ax, ay, PLINTH_TOP)], "stone", normals[i])
        (cx, cy), (dx, dy) = inner[i], inner[j]
        m.face([(ax, ay, PLINTH_TOP), (bx, by, PLINTH_TOP), (dx, dy, PLINTH_TOP), (cx, cy, PLINTH_TOP)], "stone", Z)


def build(lod):
    detail = lod
    m = Mesh()
    walls = {"front": Wall((1, 0, 0), (HL, 0, 0), True), "back": Wall((-1, 0, 0), (-HL, 0, 0), True),
             "left": Wall((0, -1, 0), (0, -HW, 0), False), "right": Wall((0, 1, 0), (0, HW, 0), False)}
    z0 = DROP if detail == 2 else PLINTH_TOP
    for name in ("left", "right"):
        w = walls[name]
        m.face([w.p(-HL, z0), w.p(HL, z0), w.p(HL, WALL_TOP), w.p(-HL, WALL_TOP)], "walls", w.n)
    for name in ("front", "back"):
        w = walls[name]
        top = [w.p(HW, WALL_TOP), w.p(0.0, under(0.0)), w.p(-HW, WALL_TOP)]
        if name == "front":
            # The door opening is cut out of the gable: the leaves and their frame cover it, so the
            # wall behind them would only take atlas space.
            dh = DOOR_HALF + (DOOR_TRIM if detail < 2 else 0.0)
            dz = DOOR_H + (DOOR_TRIM if detail < 2 else 0.0)
            m.face([w.p(-HW, z0), w.p(-dh, z0), w.p(-dh, dz), w.p(dh, dz), w.p(dh, z0), w.p(HW, z0)] + top,
                   "walls", w.n)
        else:
            m.face([w.p(-HW, z0), w.p(HW, z0)] + top, "walls", w.n)
    if detail < 2:
        plinth(m)
        for name, w in walls.items():
            half = HW if w.gable else HL
            for s in (-1.0, 1.0):
                if w.gable:
                    # The gable board wraps the corner (its outer side is the corner's side face)
                    # and its top follows the rake soffit.
                    u0, u1 = sorted((s * (half - CB), s * (half + CO)))
                    w.slab(m, u0, u1, PLINTH_TOP, (under(u0) - 0.005, under(u1) - 0.005), 0.0, CO, "trim", "flr")
                else:
                    u0, u1 = sorted((s * (half - CB), s * half))
                    w.slab(m, u0, u1, PLINTH_TOP, w.top(0.0, CO) - 0.01, 0.0, CO, "trim",
                           "fl" if s > 0 else "fr")
    roof(m, detail)
    double_doors(m, walls["front"], detail)
    hayloft(m, walls["front"], detail)
    for name, u, z, ww, hh in WINDOWS:
        window(m, walls[name], u, z, ww, hh, detail)
    if detail == 0:
        f, t = FRAME, DOOR_TRIM
        cuts = {
            "front": [(-DOOR_HALF - t, DOOR_HALF + t, DROP, DOOR_H + t),
                      (-HAY_HALF - f, HAY_HALF + f, HAY_Z0 - f, HAY_Z1 + f)],
            "back": [], "left": [], "right": [],
        }
        for name, u, z, ww, hh in WINDOWS:
            cuts[name].append((u - ww / 2 - f, u + ww / 2 + f, z - f, z + hh + f))
        for name, w in walls.items():
            battens(m, w, HW if w.gable else HL, cuts[name])
    return m.obj(f"{P}_lod{lod}")


def fill_unbaked(px, lods, size, grow=4, reach=0.75):
    """Masks texels the Cycles bake never wrote (G = 0: every slot's porosity is >= 0.05) under a
    face take that face's slot hooks and its slot's median baked AO on that LOD, then grow `grow` px
    into the unbaked background so box-filtered mips hold the values. Strips narrower than a texel
    (batten sides, course butts, the plinth ledge) rasterize no texel centre, so the bake and its
    margin skip them and they would draw with A = 0 (black, no snow). Each triangle is rasterized
    conservatively (texel centres within `reach` px of it). Islands sit about 12 px apart, so a 4 px
    growth cannot reach a neighbour's texels. Returns (pixels, number of texels written)."""
    img = px.reshape(size, size, 4).copy()
    baked = img[:, :, 1] > 0.0
    hooks = [SLOTS[s][3] for s in SLOTS]
    fill = np.zeros((size, size, 4), np.float32)
    mark = np.zeros((size, size), bool)
    for o in lods:
        me = o.data
        me.calc_loop_triangles()
        uvl = me.uv_layers["UVMap"].data
        tris = [(t.material_index, np.array([uvl[li].uv[:] for li in t.loops]) * size) for t in me.loop_triangles]
        ao = {}
        for mi, tri in tris:
            x, y = np.clip(tri.mean(0).astype(int), 0, size - 1)
            if baked[y, x]:
                ao.setdefault(mi, []).append(img[y, x, 3])
        lod_ao = float(np.median([a for v in ao.values() for a in v]))
        for mi, tri in tris:
            area2 = (tri[1, 0] - tri[0, 0]) * (tri[2, 1] - tri[0, 1]) - (tri[2, 0] - tri[0, 0]) * (tri[1, 1] - tri[0, 1])
            if abs(area2) < 1e-12:
                continue
            lo = np.clip(np.floor(tri.min(0) - reach).astype(int), 0, size - 1)
            hi = np.clip(np.ceil(tri.max(0) + reach).astype(int), 0, size - 1)
            xs, ys = np.meshgrid(np.arange(lo[0], hi[0] + 1) + 0.5, np.arange(lo[1], hi[1] + 1) + 0.5)
            inside = np.ones(xs.shape, bool)
            for i in range(3):
                a, b = tri[i], tri[(i + 1) % 3]
                e = b - a
                d = (e[0] * (ys - a[1]) - e[1] * (xs - a[0])) / np.hypot(*e) * np.sign(area2)
                inside &= d >= -reach
            iy, ix = ys[inside].astype(int), xs[inside].astype(int)
            take = ~baked[iy, ix]
            iy, ix = iy[take], ix[take]
            a_slot = float(np.median(ao[mi])) if mi in ao else lod_ao
            fill[iy, ix] = (*hooks[mi], a_slot)
            mark[iy, ix] = True
    for _ in range(grow):
        grown = mark.copy()
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            nb_mark = np.roll(mark, (dy, dx), (0, 1))
            take = nb_mark & ~grown & ~baked
            fill[take] = np.roll(fill, (dy, dx), (0, 1))[take]
            grown |= take
        mark = grown
    img[mark] = fill[mark]
    return img.reshape(-1, 4), int(mark.sum())


def main():
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(P)
    root = scene.empty(P)
    mats = []
    for s, (col, rough, tileable, _) in SLOTS.items():
        c = tuple(v * PLASTER_LIN for v in col) if tileable else col
        mats.append(materials.pbr_material(f"{P}_{s}", (*c, 1.0), rough))
    lods = [build(i) for i in range(3)]
    tris = tuple(geom.tri_count(o) for o in lods)
    print(f"{P}: tris per LOD {tris}, LOD1/LOD2 ceilings {budgets.lod_targets(tris[0], 'prop')}")
    budgets.check(tris, "prop", P)

    uv.atlas_unwrap(lods, margin=0.012)
    density = {o.name: uv.measure_texel_density(o, MASK_SIZE) for o in lods}
    print(f"{P}: masks {MASK_SIZE}² density per LOD {density}")
    signature = bake.receiver_signature(lods)
    stats = None
    if not skip_bake:
        bake.setup_cycles(samples=SAMPLES)
        scn = bpy.context.scene
        scn.world = scn.world or bpy.data.worlds.new(P)  # reset_scene leaves no world
        scn.world.light_settings.distance = AO_DIST
        ao = bake.new_image(f"{P}.ao", MASK_SIZE, non_color=True)
        # A 4 px margin stays inside the 12 px island spacing, so a later LOD's bake cannot spill
        # onto an earlier LOD's islands; bake_to hides the other LODs.
        for i, o in enumerate(lods):
            bake.bake_to(o, "AO", ao, margin=4, clear=i == 0)
        ao_png = bake.save_png(ao, str(out / f"{P}.ao.tmp.png"))
        # Hooks through one EMIT bake: each material's base colour temporarily carries
        # (snow, porosity, night) and is restored afterwards.
        keep = {m.name: tuple(m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value) for m in mats}
        for s, (_, _, _, hk) in SLOTS.items():
            bpy.data.materials[f"{P}_{s}"].node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*hk, 1.0)
        hooks = bake.new_image(f"{P}.hooks", MASK_SIZE, non_color=True)
        for i, o in enumerate(lods):
            bake.bake_emit_input([], o, "Base Color", hooks, margin=4, clear=i == 0)
        hooks_png = bake.save_png(hooks, str(out / f"{P}.hooks.tmp.png"))
        for m in mats:
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = keep[m.name]
        _, hk = bake.load_png(hooks_png)
        masks_png = bake.make_masks(MASK_SIZE, hk[:, 0], hk[:, 1], hk[:, 2], ao_png, str(out / f"{P}.masks.png"))
        os.remove(ao_png)
        os.remove(hooks_png)
        _, px = bake.load_png(masks_png)
        px, filled = fill_unbaked(px, lods, MASK_SIZE)
        bake.write_png(MASK_SIZE, px, masks_png, non_color=True)
        _, px = bake.load_png(masks_png)
        covered = px[:, 1] > 0.01  # every slot has porosity >= 0.05; empty texels stay 0
        a = px[covered, 3]
        stats = {"aoMean": float(a.mean()), "aoP05": float(np.percentile(a, 5)),
                 "nightTexels": int((px[:, 2] > 0.5).sum()), "coverage": float(covered.mean()),
                 "filledTexels": filled}
        print(f"{P}: masks {stats}")

    for o in lods:
        o.data.shade_flat()
        o.parent = root
    col = collider.collider_from(lods[2], "hull", f"{P}_collider")
    col.parent = root
    export.export_glb(root, str(out / f"{P}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, P, __file__)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=signature,
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=4,
                           aoDistanceM=AO_DIST, channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=density[f"{P}_lod0"], masksDensityPxPerM=density,
                           maskStats=stats, skipped=skip_bake)


main()
