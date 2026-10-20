"""Farm barn family, catslide variant: the builder behind barn_red_b.py (PRD §6 Variety, farm's barn slot).

A lower red board-and-batten barn whose 45° slate roof runs down past its -y wall at 22° as a catslide
over an open-fronted lean-to shed on four white posts, after the lean-to in the owner-approved ref-1
(assets-source/refs/farm; A17: pitched roof, red boards, dark slate). Against barn_red (one symmetric
45° gable, ridge 7.8 m, braced double doors) it reads from above as a broken, off-centre roof with a
louvred cupola on its ridge, and at eye level as an asymmetric gable with cross-braced doors and a
dark open shed bay along the yard side.

Plan (Blender x along the ridge, y across; the front gable with the doors faces +x, yaw 0, as
barn_red): main barn walls x ±5.0, y -1.0..3.5, eave line 3.8 m, ridge 6.05 m at y 1.25; the shed
y -3.45 (post line)..-1.0 under the catslide; the roof overhangs to y ±3.85 and the gables to x ±5.3.
Slot fit (data/tiles/farm.ron hero 0 at yaw 30: Blender +y faces edge 5's border, 0.5 m away at
barn_red's eave): the LOD2 shell, and so the box collider, stays inside barn_red's collider box
(scatter::GROUND_BOXES, x ±5.3, y ±3.85), every LOD inside barn_red's LOD0 extents (x ±5.34,
y ±3.878, measured on its GLB), and nothing grows on the +y side. So every clearance sized for
barn_red (the scatter masks, grass, walk, the bales, fence and animals) holds for this piece.

Pipeline: barn_red.py is a monolithic generator (it runs on import), so this family reuses the house
family's pipeline instead: `_house_longhouse.build_piece` runs unchanged, driven through its module
globals as `_house_slate` does (`build` is this module's builder, the fit bounds are barn_red's). The
masks bake (1024², each LOD on its own islands), the unbaked-texel fill and its zero-triangle assert,
budgets.check, the LOD2 box collider and the export are the family's, and no approved helper or
generator is edited. barn_red's own geometry steps (wall slabs, trim strips, frames, battens, braced
doors) are monolithic and are re-derived here for the asymmetric section (project.md, Variants).

Look as barn_red: the walls, doors, slate and stone sample the world-space `plaster` set tinted per
slot, the battens (every 0.5 m), slate courses, white trim and frames are geometry, trim and glass are
colour-only, and the only unique map is `masks`. Faces nobody sees are never built (the main roof's
underside over the barn interior, the wall behind the door leaves, the roof end caps behind the
bargeboards). The shed interior is built: its back wall, its end walls' inner faces and the catslide's
underside, but no floor (the meadow cap is the floor).

LODs are rebuilt from the same parameters, never decimated: LOD1 drops the battens, half the courses,
the braces, slats and bars; LOD2 is the shell, a plain roof with end caps, flat doors, posts and glass."""
import math

from mathutils import Vector

import _house_longhouse as lh
from _house import Z, Mesh
from _house_longhouse import box

DROP = -0.35
PLINTH_TOP, PLINTH_OUT = 0.35, 0.05
HL, GO = 5.0, 0.3  # wall half length (x) and gable overhang
Y_OUT, Y_IN, Y_POST = 3.5, -1.0, -3.45  # +y long wall, main -y wall (the shed's back), post line
YE = 3.85  # eave edges at y ±YE
YC = (Y_IN + Y_OUT) / 2  # ridge
EAVE = 3.8  # roof surface height at the main wall lines
TAN, TCAT = math.tan(math.radians(45.0)), math.tan(math.radians(22.0))
D = 0.2  # roof slab depth, measured vertically (so every kink lines up top and bottom)
KICK = 0.03  # slate course butt, perpendicular; applied vertically as KICK / cos(pitch)
KV45, KV22 = KICK / math.cos(math.radians(45.0)), KICK / math.cos(math.radians(22.0))
COURSES = ((12, 11, 10), (6, 5, 5), (0, 0, 0))  # (+y slope, -y slope, catslide) per LOD
POST, POST_X = 0.2, (-4.9, -1.633, 1.633, 4.9)
END_IN = 0.1  # the shed end walls' thickness (inner face at x ±(HL - END_IN))
# Battens and corner boards as barn_red: 9 cm wide, 2.5 cm proud, every 0.5 m; boards 18 cm, 3.5 cm proud.
STEP, BW, BP = 0.5, 0.09, 0.025
CB, CO = 0.18, 0.035
FRAME = 0.1
DOOR_HALF, DOOR_H, DOOR_TRIM = 1.3, 3.0, 0.16
HAY_HALF, HAY_Z0, HAY_Z1 = 0.5, 3.55, 4.75
CUP_H, CUP_ROOF = 0.5, 0.9  # cupola half size and wall height above the ridge
# Windows: (wall, u, sill z, width, height). Wall u runs along +y on the front gable, -y on the back
# gable and -x on the +y long wall.
WINDOWS = [("front", -2.25, 1.3, 0.7, 0.8), ("back", 2.25, 1.3, 0.7, 0.8), ("back", -YC, 3.9, 0.8, 0.8),
           ("side", -3.2, 1.7, 0.7, 0.75), ("side", 0.0, 1.7, 0.7, 0.75), ("side", 3.2, 1.7, 0.7, 0.75)]
# barn_red's fit, Blender (x, y): its collider box (scatter::GROUND_BOXES) for LOD2, and its LOD0 extents
# (the bargeboards and the first course butts, measured on assets/kit/p1/barn_red/barn_red.glb).
BARN_BOX = ((-5.3, 5.3), (-3.85, 3.85))
BARN_LOD0_BOUNDS = ((-5.34, 5.34), (-3.878, 3.878))

BUILT = []  # names of the LOD objects this builder made (checked after lh.build_piece returns)


def top(y):
    """Roof surface height (without course kicks) at y."""
    if y >= YC:
        return EAVE + (Y_OUT - y) * TAN
    if y >= Y_IN:
        return EAVE + (y - Y_IN) * TAN
    return EAVE - (Y_IN - y) * TCAT


def under(y):
    """Roof slab underside (soffit) at y."""
    return top(y) - D


class Wall:
    """A wall face with outward normal n and horizontal tangent t = Z x n; p(u, z, out) is the point
    u along t, at height z, `out` proud of the face; soffit(u, out) is the roof underside over it."""

    def __init__(self, n, origin, soffit):
        self.n = Vector(n)
        self.t = Z.cross(self.n)
        self.o = Vector(origin)
        self.soffit = soffit

    def p(self, u, z, out=0.0):
        return self.o + self.t * u + Z * z + self.n * out

    def slab(self, m, u0, u1, z0, z1, o0, o1, slot, faces="flrt"):
        """A box on the wall over u0..u1, z0..z1, o0..o1 proud; faces f(ront), l (u0), r (u1),
        t(op), b(ottom); never the back. z1 may be a pair (top at u0, at u1) to follow a soffit."""
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

    def frame(self, m, u, z0, w, h, detail, out, slot):
        """A picture-frame ring round an opening (u centre, z0 bottom): its front ring `out` proud,
        the outer sides (detail 0-1) and the inner reveal (detail 0); detail 2 is the ring alone."""
        f = FRAME
        u0, u1, z1 = u - w / 2, u + w / 2, z0 + h
        outer = [(u0 - f, z0 - f), (u1 + f, z0 - f), (u1 + f, z1 + f), (u0 - f, z1 + f)]
        inner = [(u0, z0), (u1, z0), (u1, z1), (u0, z1)]
        p = self.p
        for i in range(4):
            j = (i + 1) % 4
            (ou0, oz0), (ou1, oz1) = outer[i], outer[j]
            (iu0, iz0), (iu1, iz1) = inner[i], inner[j]
            m.face([p(ou0, oz0, out), p(ou1, oz1, out), p(iu1, iz1, out), p(iu0, iz0, out)], slot, self.n)
            side = (self.t * ((ou0 + ou1) / 2 - u) + Z * ((oz0 + oz1) / 2 - (z0 + z1) / 2)).normalized()
            if detail < 2:
                m.face([p(ou0, oz0, 0), p(ou1, oz1, 0), p(ou1, oz1, out), p(ou0, oz0, out)], slot, side)
            if detail == 0:
                m.face([p(iu0, iz0, out), p(iu1, iz1, out), p(iu1, iz1, 0.015), p(iu0, iz0, 0.015)], slot, -side)


def walls():
    return {
        "front": Wall((1, 0, 0), (HL, 0, 0), lambda u, out: under(u)),
        "back": Wall((-1, 0, 0), (-HL, 0, 0), lambda u, out: under(-u)),
        "side": Wall((0, 1, 0), (0, Y_OUT, 0), lambda u, out: under(Y_OUT + out)),
        "shed": Wall((0, -1, 0), (0, Y_IN, 0), lambda u, out: under(Y_IN - out)),
    }


def window(m, s, w, u, z0, width, height, detail):
    """Glass 1.5 cm proud in a white frame; four-pane cross bars at LOD0 (barn_red's window)."""
    u0, u1, z1 = u - width / 2, u + width / 2, z0 + height
    m.face([w.p(u0, z0, 0.015), w.p(u1, z0, 0.015), w.p(u1, z1, 0.015), w.p(u0, z1, 0.015)], s["glass"], w.n)
    if detail < 2:
        w.frame(m, u, z0, width, height, 2 * detail, 0.06, s["trim"])
    if detail == 0:
        w.strip(m, (u, z0), (u, z1), 0.05, 0.015, 0.04, s["trim"])
        w.strip(m, (u0, (z0 + z1) / 2), (u1, (z0 + z1) / 2), 0.05, 0.015, 0.036, s["trim"])


def x_brace(m, s, w, u0, u1, z0, z1, b, o0, o1):
    """A white border (stiles, then rails between them) round u0..u1 x z0..z1 and a cross brace
    corner to corner inside it; the second diagonal stands 3 mm less proud, so the crossing is clean."""
    for u in (u0 + b / 2, u1 - b / 2):
        w.strip(m, (u, z0), (u, z1), b, o0, o1, s["trim"])
    for z in (z0 + b / 2, z1 - b / 2):
        w.strip(m, (u0 + b, z), (u1 - b, z), b, o0, o1, s["trim"])
    w.strip(m, (u0 + b, z0 + b), (u1 - b, z1 - b), b * 0.8, o0, o1 - 0.005, s["trim"])
    w.strip(m, (u1 - b, z0 + b), (u0 + b, z1 - b), b * 0.8, o0, o1 - 0.008, s["trim"])


def double_doors(m, s, w, detail):
    """Two board leaves under the ridge in a white frame (jambs and header), each cross-braced at LOD0."""
    a, c = YC - DOOR_HALF, YC + DOOR_HALF
    if detail == 2:
        for u0, u1 in ((a, YC), (YC, c)):
            m.face([w.p(u0, DROP, 0.03), w.p(u1, DROP, 0.03), w.p(u1, DOOR_H, 0.03), w.p(u0, DOOR_H, 0.03)], s["door"], w.n)
        return
    for u0, u1, faces in ((a, YC - 0.01, "fr"), (YC + 0.01, c, "fl")):
        w.slab(m, u0, u1, DROP, DOOR_H, 0.0, 0.08, s["door"], faces)
    for u0, u1 in ((a - DOOR_TRIM, a), (c, c + DOOR_TRIM)):
        w.slab(m, u0, u1, DROP, DOOR_H, 0.0, 0.11, s["trim"], "flr")
    w.slab(m, a - DOOR_TRIM, c + DOOR_TRIM, DOOR_H, DOOR_H + DOOR_TRIM, 0.0, 0.11, s["trim"], "flrtb")
    if detail == 0:
        for u0, u1 in ((a + 0.02, YC - 0.03), (YC + 0.03, c - 0.02)):
            x_brace(m, s, w, u0, u1, 0.05, DOOR_H - 0.02, 0.13, 0.08, 0.105)


def hayloft(m, s, w, detail):
    """The hayloft door above the doors: a board leaf 3 cm proud in a white frame, cross-braced at LOD0."""
    u0, u1 = YC - HAY_HALF, YC + HAY_HALF
    m.face([w.p(u0, HAY_Z0, 0.03), w.p(u1, HAY_Z0, 0.03), w.p(u1, HAY_Z1, 0.03), w.p(u0, HAY_Z1, 0.03)], s["door"], w.n)
    if detail < 2:
        w.frame(m, YC, HAY_Z0, 2 * HAY_HALF, HAY_Z1 - HAY_Z0, 2 * detail, 0.06, s["trim"])
    if detail == 0:
        w.strip(m, (u0 + 0.08, HAY_Z0 + 0.08), (u1 - 0.08, HAY_Z1 - 0.08), 0.09, 0.03, 0.05, s["trim"])
        w.strip(m, (u1 - 0.08, HAY_Z0 + 0.08), (u0 + 0.08, HAY_Z1 - 0.08), 0.09, 0.03, 0.047, s["trim"])


def battens(m, s, w, ua, ub, openings, z_start, ref):
    """Vertical battens over one wall from ua to ub, at half steps from `ref` (so openings centred on
    whole steps fall between them), cut round the openings (u0, u1, z0, z1) and running up to the
    soffit. Cut ends abut a more proud frame, so they need no end faces (barn_red's battens)."""
    k = math.ceil((ua + BW / 2 - ref - STEP / 2) / STEP)
    u = ref + STEP / 2 + k * STEP
    while u + BW / 2 <= ub:
        u0, u1 = u - BW / 2, u + BW / 2
        segs, z = [], z_start
        for a, b, z0, z1 in sorted(openings, key=lambda o: o[2]):
            if a < u1 and b > u0:
                if z0 > z:
                    segs.append((z, z0))
                z = max(z, z1)
        segs.append((z, None))
        for za, zb in segs:
            if zb is None:
                tf = (w.soffit(u0, BP), w.soffit(u1, BP))
                tb = (w.soffit(u0, 0.0), w.soffit(u1, 0.0))
            else:
                tf = tb = (zb, zb)
            if min(tf) - za < 0.15:
                continue
            p = w.p
            m.face([p(u0, za, BP), p(u1, za, BP), p(u1, tf[1], BP), p(u0, tf[0], BP)], s["walls"], w.n)
            m.face([p(u0, za, 0.0), p(u0, za, BP), p(u0, tf[0], BP), p(u0, tb[0], 0.0)], s["walls"], -w.t)
            m.face([p(u1, za, 0.0), p(u1, tb[1], 0.0), p(u1, tf[1], BP), p(u1, za, BP)], s["walls"], w.t)
        u += STEP


def course_pts(a, b, n, kv):
    """Profile points up one roof segment from a (lower) to b (upper), b excluded: each course's butt
    `kv` above the plane, rising flush to the next butt; n = 0 is the plain segment start."""
    if n == 0:
        return [a]
    pts = []
    for k in range(n):
        pts.append(a + (b - a) * (k / n) + Vector((0.0, kv)))
        if k < n - 1:
            pts.append(a + (b - a) * ((k + 1) / n))
    return pts


def roof_profile(lod):
    """Closed counter-clockwise (y, z) section, ridge on the x axis: the +y eave's fascia, the +y slope
    up to the ridge, the -y slope down to the kink over the -y wall, the catslide to the -y eave and
    its fascia, then the underside (eave, kink, ridge). Returns (points, index of the -y fascia's
    lower end): every edge from there on is underside, built separately where it can be seen."""
    n1, n2, n3 = COURSES[lod]
    v = lambda y: Vector((y, top(y)))  # noqa: E731
    e_plus, ridge, kink, e_minus = v(YE), v(YC), v(Y_IN), v(-YE)
    plus = course_pts(e_plus, ridge, n1, KV45)
    minus = course_pts(kink, ridge, n2, KV45)
    cat = course_pts(e_minus, kink, n3, KV22)
    pts = [Vector((YE, under(YE)))] + plus + [ridge + Vector((0.0, KV45 if n1 else 0.0))] + minus[::-1]
    if n3:
        pts.append(kink + Vector((0.0, KV22)))
    pts += cat[::-1]
    low = len(pts)
    pts += [Vector((-YE, under(-YE))), Vector((Y_IN, under(Y_IN))), Vector((YC, under(YC)))]
    return pts, low


def roof(m, s, lod):
    """The slab swept along x over the gable overhangs with its courses and white fascias, the visible
    undersides (the +y eave strip, the gable overhangs over the main barn, and the catslide's whole
    underside over the shed), and either end caps (LOD2) or white bargeboards over them (LOD0-1)
    and a split slate ridge cap either side of the cupola."""
    pts, low = roof_profile(lod)
    x0, x1 = -HL - GO, HL + GO
    used = pts if lod == 2 else pts[:low + 1]  # the underside points only close the LOD2 end caps
    r0 = [m.bm.verts.new((x0, q.x, q.y)) for q in used]
    r1 = [m.bm.verts.new((x1, q.x, q.y)) for q in used]
    for i in range(low):
        j = i + 1
        d = pts[j] - pts[i]
        out = Vector((0.0, d.y, -d.x))  # outward of a counter-clockwise (y, z) edge
        f = m.bm.faces.new((r0[i], r0[j], r1[j], r1[i]))
        f.material_index = m.idx[s["trim"] if i in (0, low - 1) else s["roof"]]
        f.normal_update()
        if f.normal.dot(out) < 0:
            f.normal_flip()
    if lod == 2:
        for ring, nx in ((r0, -1.0), (r1, 1.0)):
            f = m.bm.faces.new(ring)
            f.material_index = m.idx[s["roof"]]
            f.normal_update()
            if f.normal.x * nx < 0:
                f.normal_flip()
    down = -Z
    # Undersides: the catslide over the shed and its overhangs, the +y eave strip, the gable overhangs.
    m.face([(x0, -YE, under(-YE)), (x1, -YE, under(-YE)), (x1, Y_IN, under(Y_IN)), (x0, Y_IN, under(Y_IN))], s["roof"], down)
    m.face([(x0, Y_OUT, under(Y_OUT)), (x1, Y_OUT, under(Y_OUT)), (x1, YE, under(YE)), (x0, YE, under(YE))], s["roof"], down)
    for xa, xb in ((HL, x1), (x0, -HL)):
        for ya, yb in ((Y_IN, YC), (YC, Y_OUT)):
            m.face([(xa, ya, under(ya)), (xb, ya, under(ya)), (xb, yb, under(yb)), (xa, yb, under(yb))], s["roof"], down)
    if lod == 2:
        return
    # Ridge cap: two slate boards 0.2 m down each side, either side of the cupola.
    apex = top(YC) + KV45 + 0.05
    for xa, xb in ((x0, -CUP_H), (CUP_H, x1)):
        for sg in (-1.0, 1.0):
            side = top(YC + sg * 0.2) + KV45 + 0.02
            m.face([(xa, YC, apex), (xb, YC, apex), (xb, YC + sg * 0.2, side), (xa, YC + sg * 0.2, side)], s["roof"],
                   Vector((0.0, sg, 1.0)))
    # Bargeboards: a 4 cm board against each end cap, from 6 cm below the underside to over the
    # course butts, along the three rakes, with an end face at each eave.
    ys = (YE, YC, Y_IN, -YE)
    hi = [top(y) + KV45 + 0.01 for y in ys]
    lo = [under(y) - 0.06 for y in ys]
    for xs in (-1.0, 1.0):
        xa, xb = xs * (HL + GO), xs * (HL + GO + 0.04)
        X = Vector((xs, 0.0, 0.0))
        for i in range(3):
            ya, yb = ys[i], ys[i + 1]
            m.face([(xb, ya, lo[i]), (xb, yb, lo[i + 1]), (xb, yb, hi[i + 1]), (xb, ya, hi[i])], s["trim"], X)
            m.face([(xa, ya, hi[i]), (xb, ya, hi[i]), (xb, yb, hi[i + 1]), (xa, yb, hi[i + 1])], s["trim"], Z)
            m.face([(xa, ya, lo[i]), (xb, ya, lo[i]), (xb, yb, lo[i + 1]), (xa, yb, lo[i + 1])], s["trim"], -Z)
        for y, zl, zh in ((ys[0], lo[0], hi[0]), (ys[-1], lo[-1], hi[-1])):
            m.face([(xa, y, zl), (xb, y, zl), (xb, y, zh), (xa, y, zh)], s["trim"], Vector((0.0, math.copysign(1.0, y), 0.0)))


def cupola(m, s, lod):
    """A louvred cupola on the ridge at x 0: red board walls cut to the roof slopes (their feet 5 cm
    inside the slab), a dark louvre panel in a white frame on each face (slats at LOD0), a white
    fascia and a slate pyramid cap."""
    h, ridge = CUP_H, top(YC)
    zt = ridge + CUP_ROOF
    ya, yb = YC - h, YC + h
    for sg, y in ((-1.0, ya), (1.0, yb)):
        zb = top(y) - 0.05
        m.face([(-h, y, zb), (h, y, zb), (h, y, zt), (-h, y, zt)], s["walls"], Vector((0.0, sg, 0.0)))
    for sg in (-1.0, 1.0):
        x = sg * h
        m.face([(x, ya, top(ya) - 0.05), (x, YC, ridge - 0.05), (x, yb, top(yb) - 0.05), (x, yb, zt), (x, ya, zt)],
               s["walls"], Vector((sg, 0.0, 0.0)))
    faces = [Wall((1, 0, 0), (h, YC, 0), None), Wall((-1, 0, 0), (-h, YC, 0), None),
             Wall((0, 1, 0), (0, yb, 0), None), Wall((0, -1, 0), (0, ya, 0), None)]
    lz0, lz1, lw = ridge + 0.25, zt - 0.12, 0.56  # the frame's foot clears the ridge cap's ends
    for w in faces:
        m.face([w.p(-lw / 2, lz0, 0.01), w.p(lw / 2, lz0, 0.01), w.p(lw / 2, lz1, 0.01), w.p(-lw / 2, lz1, 0.01)], s["roof"], w.n)
        if lod < 2:
            w.frame(m, 0.0, lz0, lw, lz1 - lz0, 2, 0.04, s["trim"])
        if lod == 0:
            for t in (1 / 3, 2 / 3):
                z = lz0 + (lz1 - lz0) * t
                w.strip(m, (-lw / 2, z), (lw / 2, z), 0.04, 0.01, 0.035, s["trim"])
    o = h + 0.12
    zr = zt + (0.08 if lod < 2 else 0.0)
    if lod < 2:
        for w in faces:
            w.slab(m, -o, o, zt, zr, 0.0, 0.12, s["trim"], "f")
    corners = [(o, YC - o), (o, YC + o), (-o, YC + o), (-o, YC - o)]
    m.face([(x, y, zt) for x, y in corners], s["roof"], -Z)
    apex = (0.0, YC, zr + 0.55)
    for i in range(4):
        (ax, ay), (bx, by) = corners[i], corners[(i + 1) % 4]
        out = Vector(((ax + bx) / 2, (ay + by) / 2 - YC, 0.4))
        m.face([(ax, ay, zr), (bx, by, zr), apex], s["roof"], out)
    if lod == 0:
        # A short finial over the cap.
        box(m, (0.0, YC, apex[2] + 0.12), Vector((1, 0, 0)), Vector((0, 1, 0)), Z, (0.05, 0.05, 0.3), s["trim"], drop=(-Z,))


def shed(m, s, lod):
    """The lean-to's four white posts on the post line, the plate under the catslide's edge, knee braces
    at LOD0, the shed's back wall (the main barn's -y wall) and the inner faces of its end walls."""
    zp = under(Y_POST + POST / 2) - 0.25  # plate bottom
    X, Y = Vector((1, 0, 0)), Vector((0, 1, 0))
    for x in POST_X:
        box(m, (x, Y_POST, (DROP + zp) / 2), X, Y, Z, (POST, POST, zp - DROP), s["trim"], drop=(Z, -Z))
    y0, y1 = Y_POST - POST / 2, Y_POST + POST / 2
    for y, n in ((y0, -Y), (y1, Y)):
        m.face([(-HL, y, zp), (HL, y, zp), (HL, y, under(y)), (-HL, y, under(y))], s["trim"], n)
    m.face([(-HL, y0, zp), (HL, y0, zp), (HL, y1, zp), (-HL, y1, zp)], s["trim"], -Z)
    for xs in (-1.0, 1.0):
        x = xs * HL
        m.face([(x, y0, zp), (x, y1, zp), (x, y1, under(y1)), (x, y0, under(y0))], s["trim"], X * xs)
    if lod == 0:
        # Knee braces, 0.6 m each way, from each post's side up to the plate (one inward at the corners).
        r = 0.6
        for x in POST_X:
            for sg in (-1.0, 1.0):
                if abs(x + sg * r) > HL - 0.2:
                    continue
                a = Vector((x + sg * POST / 2, Y_POST, zp - r))
                b = Vector((x + sg * (POST / 2 + r), Y_POST, zp))
                ax = (b - a).normalized()
                box(m, (a + b) / 2, ax, Y, ax.cross(Y), ((b - a).length + 0.1, 0.1, 0.1), s["trim"], drop=(ax, -ax))
    # The shed's back wall (the main barn's -y wall, in the shed) and the end walls' inner faces.
    xi = HL - END_IN
    m.face([(-xi, Y_IN, DROP), (xi, Y_IN, DROP), (xi, Y_IN, under(Y_IN)), (-xi, Y_IN, under(Y_IN))], s["walls"], -Y)
    for xs in (-1.0, 1.0):
        x = xs * xi
        m.face([(x, y1, DROP), (x, Y_IN, DROP), (x, Y_IN, under(Y_IN)), (x, y1, under(y1))], s["walls"], -X * xs)
    if lod < 2:
        # The lean-to end windows seen from inside.
        for xs in (-1.0, 1.0):
            x = xs * (xi - 0.015)
            c, wv, z0, hv = -2.25, 0.7, 1.3, 0.8
            m.face([(x, c - wv / 2, z0), (x, c + wv / 2, z0), (x, c + wv / 2, z0 + hv), (x, c - wv / 2, z0 + hv)],
                   s["glass"], -X * xs)


def plinth(m, s):
    """The stone plinth along the closed side of the barn (front gable, +y wall, back gable): each run's
    outer face and its ledge over the wall line, mitred at the two corners, with end faces at the
    post line."""
    chain = [(HL, Y_POST + POST / 2), (HL, Y_OUT), (-HL, Y_OUT), (-HL, Y_POST + POST / 2)]
    nrm = []
    for i in range(3):
        (ax, ay), (bx, by) = chain[i], chain[i + 1]
        ln = math.hypot(bx - ax, by - ay)
        nrm.append(((by - ay) / ln, -(bx - ax) / ln))
    outer = []
    for i, (px, py) in enumerate(chain):
        ns = [nrm[j] for j in (i - 1, i) if 0 <= j < 3]
        outer.append((px + sum(n[0] for n in ns) * PLINTH_OUT, py + sum(n[1] for n in ns) * PLINTH_OUT))
    for i in range(3):
        (ax, ay), (bx, by) = outer[i], outer[i + 1]
        nv = Vector((*nrm[i], 0.0))
        m.face([(ax, ay, DROP), (bx, by, DROP), (bx, by, PLINTH_TOP), (ax, ay, PLINTH_TOP)], s["stone"], nv)
        m.face([(ax, ay, PLINTH_TOP), (bx, by, PLINTH_TOP), (*chain[i + 1], PLINTH_TOP), (*chain[i], PLINTH_TOP)], s["stone"], Z)
    for i in (0, 3):
        (px, py), (ox, oy) = chain[i], outer[i]
        m.face([(px, py, DROP), (ox, oy, DROP), (ox, oy, PLINTH_TOP), (px, py, PLINTH_TOP)], s["stone"], Vector((0, -1, 0)))


def build(spec, lod, slots, mats):
    """One LOD of the barn (module docstring). `spec["slots"]` maps the short slot names to materials."""
    s = spec["slots"]
    m = Mesh(slots)
    W = walls()
    z0 = PLINTH_TOP if lod < 2 else DROP
    yl = Y_POST + POST / 2  # the closed walls end at the posts' inner faces
    # +y long wall, then both gables under the roof's underside; the door opening is cut out of the
    # front gable (the leaves and their frame cover it).
    m.face([(HL, Y_OUT, z0), (-HL, Y_OUT, z0), (-HL, Y_OUT, under(Y_OUT)), (HL, Y_OUT, under(Y_OUT))], s["walls"],
           Vector((0, 1, 0)))
    roof_line = [(Y_OUT, under(Y_OUT)), (YC, under(YC)), (Y_IN, under(Y_IN)), (yl, under(yl))]
    dh = DOOR_HALF + (DOOR_TRIM if lod < 2 else 0.0)
    dz = DOOR_H + (DOOR_TRIM if lod < 2 else 0.0)
    front = [(yl, z0), (YC - dh, z0), (YC - dh, dz), (YC + dh, dz), (YC + dh, z0), (Y_OUT, z0)] + roof_line
    m.face([(HL, y, z) for y, z in front], s["walls"], Vector((1, 0, 0)))
    m.face([(-HL, y, z) for y, z in [(yl, z0), (Y_OUT, z0)] + roof_line], s["walls"], Vector((-1, 0, 0)))
    if lod < 2:
        plinth(m, s)
        for name in ("front", "back"):
            w = W[name]
            sg = 1.0 if name == "front" else -1.0
            u0, u1 = sorted((sg * (Y_OUT - CB), sg * (Y_OUT + CO)))
            w.slab(m, u0, u1, PLINTH_TOP, (w.soffit(u0, 0) - 0.005, w.soffit(u1, 0) - 0.005), 0.0, CO, s["trim"], "flr")
        w = W["side"]
        for sg in (-1.0, 1.0):
            u0, u1 = sorted((sg * (HL - CB), sg * HL))
            w.slab(m, u0, u1, PLINTH_TOP, w.soffit(0.0, CO) - 0.01, 0.0, CO, s["trim"], "fl" if sg > 0 else "fr")
    roof(m, s, lod)
    cupola(m, s, lod)
    shed(m, s, lod)
    double_doors(m, s, W["front"], lod)
    hayloft(m, s, W["front"], lod)
    for name, u, z, ww, hh in WINDOWS:
        window(m, s, W[name], u, z, ww, hh, lod)
    if lod == 0:
        f, t = FRAME, DOOR_TRIM
        cuts = {"front": [(YC - DOOR_HALF - t, YC + DOOR_HALF + t, DROP, DOOR_H + t),
                          (YC - HAY_HALF - f, YC + HAY_HALF + f, HAY_Z0 - f, HAY_Z1 + f)],
                "back": [], "side": []}
        for name, u, z, ww, hh in WINDOWS:
            cuts[name].append((u - ww / 2 - f, u + ww / 2 + f, z - f, z + hh + f))
        battens(m, s, W["front"], yl + 0.02, Y_OUT - CB - 0.02, cuts["front"], PLINTH_TOP, YC)
        battens(m, s, W["back"], -Y_OUT + CB + 0.02, -yl - 0.02, cuts["back"], PLINTH_TOP, -YC)
        battens(m, s, W["side"], -HL + CB + 0.02, HL - CB - 0.02, cuts["side"], PLINTH_TOP, 0.0)
    o = m.obj(f"{spec['piece']}_lod{lod}", mats)
    BUILT.append(o.name)
    return o


def build_piece(spec, generator):
    """Run the house family's pipeline (`_house_longhouse.build_piece`) with this builder and barn_red's
    fit bounds (module docstring)."""
    lh.build = build
    lh.COTTAGE_BOUNDS = BARN_BOX
    lh.COTTAGE_LOD0_BOUNDS = BARN_LOD0_BOUNDS
    BUILT.clear()
    root, lods = lh.build_piece(spec, generator)
    assert [o.name for o in lods] == BUILT, ("_house_longhouse.build_piece did not use this builder", BUILT)
    return root, lods
