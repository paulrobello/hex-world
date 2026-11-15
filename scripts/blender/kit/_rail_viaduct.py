"""Rail viaduct family: rail_viaduct_l1, _l2 and _l3 (P4, rail at levels; owner-approved references
assets-source/refs/rail_viaduct ref-2 for the form and ref-4 for the stone and deck, 2026-11-13). A stone
viaduct of three round arches on slightly tapering piers carries the single track of
`rail_straight_center` from edge 3 to edge 0 with its deck N levels over the tile's ground, so the rail
top stands at N x LEVEL_STEP + `_rail.RAIL_TOP` and meets the ordinary rail of a neighbour standing at
level N (hex-core `LaneSpec.deck`, `deck_meet`: a cliff seam). One design for every height (owner RL-Q4):
the arches, the deck and the parapets are fixed, and only the piers grow with N. No walking on the deck
(RL-Q3): the piers are obstacles and no up-facing face of the piece is ground.

Frame: Blender x runs along the track (edge 3 at -x, edge 0 at +x), y across it, z up; runtime
(x, y, z) = Blender (x, z, -y). The pivot is the hex centre on the tile's ground.

- Track: `_rail.track` along `_rail.straight_frames` exactly as rail_straight_center builds it (8, 2, 2
  segments; full sleepers, sleeper tops, none), checked unlifted against `_rail.section()` and
  `_rail.interior_errors`, then lifted by the deck height, so its section at both lane borders is the
  rail section raised by N x LEVEL_STEP. Its slots are renamed (`viaduct_*`), so runtime ground rules
  can tell the deck from rail on the ground.
- Stone: face walls on y = +-W with three tombstone openings, the arch rings of `_tunnel_portal`'s
  voussoirs, the barrel soffits, battered pier sides, a projecting string course, parapets with a
  coping, and closed ends 2 mm inside the lane borders (nothing but the track touches a border, and
  no up-facing face sits on the tile's ring).
- Texture: the stone samples `_tunnel_portal`'s shared islands, built and baked by that helper's own
  functions with its globals unchanged (the approved portal's granite, buff stones, lime mortar,
  lichen, runoff and foot damp), plus one extra non-true-scale island that carries the track's masks
  (`_rail.masks_field("straight")`, folded about x = 0). The bake depends on the templates alone, so
  the three pieces share it (cached by receiver signature in target/, outside every stage folder).
"""
from __future__ import annotations

import json
import math
import os
import pathlib
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector
from mathutils.geometry import delaunay_2d_cdt

import _rail
import _tunnel_portal as tp
from lib import bake, budgets, collider, export, geom, hexgeo, materials, scene

STEP = hexgeo.LEVEL_STEP
APOTHEM = hexgeo.APOTHEM
X_END = APOTHEM - 0.002  # stone stops 2 mm inside the lane borders: only the track meets them
W = 3.0  # face wall half width
PAR_IN = 2.65  # parapet inner face: outside the train's 2.5 m swept half width
SWEPT = 2.5
R_ARCH = 2.3  # intrados radius (semicircular arches)
PIER = 2.4  # inner pier width at the springing
X_C = (-(2 * R_ARCH + PIER), 0.0, 2 * R_ARCH + PIER)  # arch centres along x
BATTER = 1.0 / 40.0  # pier sides lean in going up (the opening narrows toward the ground)
BURY = tp.BURY  # 0.3: under the meadow relief's -0.25 m dips
# Deck section, metres from the deck height H = N x LEVEL_STEP: the string course, the parapet top
# (the coping's foot) and the coping (0.43 wide: the portal's cap island, CAP_OUT + CAP_BACK).
SC_LO, SC_HI, SC_OUT = -0.55, -0.33, 0.08
DECK_Z = _rail.SINK_Z - 0.01  # the end cap's top under the ballast's sink edge
PAR_TOP = 0.95
COP_H = tp.TOP - tp.FACE_TOP - tp.LOW_H  # 0.16, the cap course
COP_IN, COP_OUT = PAR_IN - 0.04, W + 0.04
SPANDREL_MIN = 0.2  # stone over the ring's crown, under the string course
SPRING_DROP = -SC_LO + SPANDREL_MIN + tp.RING + R_ARCH  # H - springing height
assert abs((COP_OUT - COP_IN) - (tp.CAP_OUT + tp.CAP_BACK)) < 1e-9
assert PAR_IN > SWEPT and STEP - SPRING_DROP > 0.3, "l1 keeps a visible pier"
LIFT = 0.064  # track UV metres per metre of height (as _rail.UV_LIFT on its 32 m map)
TRACK_PX_PER_M = 24  # atlas px per metre on the track masks island (12 masks px/m)
TRACK_T = _rail.SINK_T + 0.1

# Per LOD: voussoirs per arch, map scale of the big stone faces, coping segments, track segments,
# sleepers ("full", "top", None), whether the rings (and the string course) are drawn.
# "scale" maps the faces and the parapets' outer faces, "minor" the barrels, the pier sides and the
# parapets' inner faces (cells span scale x the panel); "string" draws the string course (an 8 cm
# ledge, two pixels at LOD1's 150 m), "under" the undersides of the coping and the string course
# (seen only from below the deck).
LOD = [
    {"vous": 11, "scale": 1.0, "minor": 1.0, "cop": 28, "track": 8, "sleepers": "full", "ring": True, "string": True,
     "under": True},
    {"vous": 7, "scale": 1.0, "minor": 2.0, "cop": 4, "track": 2, "sleepers": "top", "ring": True, "string": False,
     "under": False},
    {"vous": 5, "scale": 2.5, "minor": 5.0, "cop": 1, "track": 2, "sleepers": None, "ring": False, "string": False,
     "under": False},
]
TRACK_SLOTS = {_rail.BALLAST: "viaduct_ballast", _rail.SLEEPER: "viaduct_sleeper", _rail.STEEL: "viaduct_steel",
               _rail.HEAD: "viaduct_head"}
BAKE_CACHE = pathlib.Path(__file__).resolve().parents[3] / "target" / "rail_viaduct_bake"
BAKE_SEED = "rail_viaduct"  # one template set (material names and noise) for every height


def heights(n):
    h = n * STEP
    return {"H": h, "S": h - SPRING_DROP, "SC_LO": h + SC_LO, "SC_HI": h + SC_HI, "PAR_TOP": h + PAR_TOP,
            "DECK": h + DECK_Z}


def hw(z, s):
    """The opening's half width at height z (springing s): R at the springing, narrower below."""
    return R_ARCH - BATTER * (s - z)


# ---------------------------------------------------------------------------------------------
# 2D helpers
def inside(p, poly):
    x, y = p
    c = False
    for (x0, y0), (x1, y1) in zip(poly, poly[1:] + poly[:1]):
        if (y0 > y) != (y1 > y) and x < x0 + (y - y0) * (x1 - x0) / (y1 - y0):
            c = not c
    return c


def course_rows(z_lo, z_hi, anchor, top_band, s):
    """Rows (z0, z1, island, v origin) from z_lo to z_hi. At true scale the courses are CH tall with a
    joint at `anchor`: the foot band on the course holding z = 0, panel rows of PH above it (and the
    buried strip below), and with `top_band` the band under the course that ends at `anchor`. At a
    coarser scale, panel rows of PH x s with a joint at `anchor`."""
    ch, ph = tp.CH, tp.PH
    rows = []
    if s == 1.0:
        k = math.ceil(anchor / ch)
        zf = anchor - k * ch
        top_start = anchor - ch if top_band else math.inf
        rows.append((z_lo, zf, "panel", zf - ph))
        rows.append((zf, zf + ch, "band_foot", zf))
        z = zf + ch
        while z < min(top_start, z_hi) - 1e-9:
            rows.append((z, min(z + ph, top_start), "panel", z))
            z += ph
        if top_band:
            rows.append((top_start, anchor, "band_top", top_start))
    else:
        p = ph * s
        r = math.floor((z_lo - anchor) / p)
        while anchor + r * p < z_hi - 1e-9:
            rows.append((anchor + r * p, anchor + (r + 1) * p, "panel", anchor + r * p))
            r += 1
    out = []
    for z0, z1, name, vo in rows:
        a, b = max(z0, z_lo), min(z1, z_hi)
        if b > a + 1e-6:
            out.append((a, b, name, vo))
    return out


def columns(a_lo, a_hi, ri, s):
    """Column edges of row ri over [a_lo, a_hi]: cells PW x s wide, each row's origin shifted."""
    pw = tp.PW * s
    origin = a_lo - pw + ((0.83 * (ri + 1)) % tp.PW) * s
    while origin + pw <= a_lo:
        origin += pw
    cols = []
    c = origin
    while c < a_hi - 1e-9:
        cols.append((c, c + pw))
        c += pw
    return cols


class Region:
    """A planar stone surface in its own (a, z) coordinates: P(a, z) -> Blender point; `flip` maps
    the panel u from the column's right edge (u runs to the viewer's right)."""

    def __init__(self, P, expect, flip, s):
        self.P, self.expect, self.flip, self.s = P, expect, flip, s

    def uv(self, isl, a, z, col, vo):
        s = self.s
        u = (col[1] - a) / s if self.flip else (a - col[0]) / s
        return isl.uv(u, (z - vo) / s)

    def cell(self, b, isl, pts, col, row):
        name, vo = row[2], row[3]
        b.face([self.P(a, z) for a, z in pts], [self.uv(isl[name], a, z, col, vo) for a, z in pts], "stone",
               self.expect(pts) if callable(self.expect) else self.expect)


def rect_region(b, isl, reg, a_lo, a_hi, rows, extra_z=()):
    """Quads of every (row, column) cell of a rectangle, split at `extra_z` (polyline kinks)."""
    for ri, row in enumerate(rows):
        z0, z1 = row[0], row[1]
        zs = sorted({z0, z1} | {z for z in extra_z if z0 + 1e-6 < z < z1 - 1e-6})
        for col in columns(a_lo, a_hi, ri, reg.s):
            ca, cb = max(col[0], a_lo), min(col[1], a_hi)
            if cb <= ca + 1e-6:
                continue
            for za, zb in zip(zs, zs[1:]):
                reg.cell(b, isl, [(ca, za), (cb, za), (cb, zb), (ca, zb)], col, row)


def holed_region(b, isl, reg, a_lo, a_hi, rows, holes):
    """A rectangle minus `holes`, triangulated (constrained Delaunay) with every row line, every
    column edge and every hole edge as a constraint, so each triangle lies in one cell."""
    z_lo, z_hi = rows[0][0], rows[-1][1]
    pts, edges = [], []

    def poly(p, closed=True):
        base = len(pts)
        pts.extend(p)
        n = len(p)
        for i in range(n if closed else n - 1):
            edges.append((base + i, base + (i + 1) % n))

    poly([(a_lo, z_lo), (a_hi, z_lo), (a_hi, z_hi), (a_lo, z_hi)])
    for h in holes:
        poly(h)
    cols_of = []
    for ri, row in enumerate(rows):
        poly([(a_lo, row[0]), (a_hi, row[0])], closed=False)
        cols = columns(a_lo, a_hi, ri, reg.s)
        cols_of.append(cols)
        for c0, _c1 in cols:
            if a_lo + 1e-6 < c0 < a_hi - 1e-6:
                poly([(c0, row[0]), (c0, row[1])], closed=False)
    vs, _e, fs, _ov, _oe, _of = delaunay_2d_cdt([Vector(p) for p in pts], edges, [], 0, 1e-5)
    n_out = 0
    for f in fs:
        tri = [(vs[i].x, vs[i].y) for i in f]
        area = (tri[1][0] - tri[0][0]) * (tri[2][1] - tri[0][1]) - (tri[2][0] - tri[0][0]) * (tri[1][1] - tri[0][1])
        if abs(area) < 2e-6:
            continue
        c = (sum(p[0] for p in tri) / 3, sum(p[1] for p in tri) / 3)
        if any(inside(c, h) for h in holes):
            continue
        ri = next(i for i, r in enumerate(rows) if r[0] - 1e-6 <= c[1] <= r[1] + 1e-6)
        col = next(cc for cc in cols_of[ri] if cc[0] - 1e-6 <= c[0] <= cc[1] + 1e-6)
        reg.cell(b, isl, tri, col, rows[ri])
        n_out += 1
    return n_out


# ---------------------------------------------------------------------------------------------
# Arches
class ArcStone:
    """A voussoir of the arch centred at (xc, springing s) on the face at y = side x W, from angle
    t0 to t1 (f along it, r radial from the intrados, dd toward the viewer), for tp.ring_stone."""

    def __init__(self, xc, s, sgn, t0, t1):
        self.xc, self.s, self.sgn, self.t0, self.t1 = xc, s, sgn, t0, t1
        self.front = (0.0, float(sgn), 0.0)

    def t(self, f):
        return self.t0 + f * (self.t1 - self.t0)

    def __call__(self, f, r, dd):
        t = self.t(f)
        return (self.xc + (R_ARCH + r) * math.cos(t), self.sgn * (W + dd), self.s + (R_ARCH + r) * math.sin(t))

    def out(self, f):
        t = self.t(f)
        return (math.cos(t), 0.0, math.sin(t))

    def inward(self, f):
        t = self.t(f)
        return (-math.cos(t), 0.0, -math.sin(t))

    def side(self, f):
        t = self.t(f)
        tg = (-math.sin(t), 0.0, math.cos(t))
        return tuple(-c for c in tg) if f == 0.0 else tg


def arch_angles(n):
    return [math.pi * i / n for i in range(n + 1)]


def opening(xc, s, n, ring):
    """The face wall's hole for one arch, in (x, z): the opening from below the ground up the battered
    pier sides, then the extrados polyline (`ring`) or the intrados (no ring)."""
    zb = -BURY
    r = R_ARCH + tp.RING if ring else R_ARCH
    arc = [(xc + r * math.cos(t), s + r * math.sin(t)) for t in reversed(arch_angles(n))]
    if ring:
        return [(xc - hw(zb, s), zb), (xc - R_ARCH, s)] + arc + [(xc + R_ARCH, s), (xc + hw(zb, s), zb)]
    return [(xc - hw(zb, s), zb)] + arc + [(xc + hw(zb, s), zb)]


# ---------------------------------------------------------------------------------------------
# The stone of one LOD
PARTS = {}  # stone triangles per part per LOD (the budget report)


def build_stone(name, n, lod, isl, mats):
    d = LOD[lod]
    hh = heights(n)
    H, S = hh["H"], hh["S"]
    sc, mi = d["scale"], d["minor"]
    b = tp.Builder(("stone",))
    ring = d["ring"]
    yin = W - tp.SOFFIT if ring else W  # the barrel's half width between the ring soffits
    PARTS[lod] = {}

    def mark(part):
        tris = sum(len(f.verts) - 2 for f in b.bm.faces)
        PARTS[lod][part] = tris - sum(PARTS[lod].values())

    # Face walls with the three openings.
    face_top = hh["SC_LO"]
    rows = course_rows(-BURY, face_top, face_top, True, sc)
    holes = [opening(xc, S, d["vous"], ring) for xc in X_C]
    for side in (1, -1):
        reg = Region(lambda a, z, side=side: (a, side * W, z), (0.0, side, 0.0), side > 0, sc)
        holed_region(b, isl, reg, -X_END, X_END, rows, holes)

    mark("face")
    # Rings.
    if ring:
        ths = arch_angles(d["vous"])
        for xc in X_C:
            for side in (1, -1):
                for i in range(d["vous"]):
                    am = ArcStone(xc, S, side, ths[i], ths[i + 1])
                    end = i in (0, d["vous"] - 1)
                    tp.ring_stone(b, am, isl, "v0" if i % 2 == 0 else "v1", 0.75, tp.RING, tp.V_PROUD, 1, end, True)

    mark("rings")
    # Barrel soffits: u across the viaduct, v along the arc (courses parallel to the vault's axis).
    ths = arch_angles(d["vous"])
    g_tot = math.pi * R_ARCH
    gs = [R_ARCH * t for t in ths]
    for xc in X_C:
        def bar(a, g, xc=xc):
            t = g / R_ARCH
            k = min(int(t / (math.pi / d["vous"])), d["vous"] - 1)
            t0, t1 = ths[k], ths[k + 1]
            f = (t - t0) / (t1 - t0)
            p0 = (xc + R_ARCH * math.cos(t0), S + R_ARCH * math.sin(t0))
            p1 = (xc + R_ARCH * math.cos(t1), S + R_ARCH * math.sin(t1))
            return (p0[0] + f * (p1[0] - p0[0]), a, p0[1] + f * (p1[1] - p0[1]))

        def inward(pts, xc=xc):
            t = sum(g for _a, g in pts) / len(pts) / R_ARCH
            return (-math.cos(t), 0.0, -math.sin(t))
        reg = Region(bar, inward, False, mi)
        brows = []
        g = 0.0
        while g < g_tot - 1e-9:
            brows.append((g, min(g + tp.PH * mi, g_tot), "panel", g))
            g += tp.PH * mi
        rect_region(b, isl, reg, -yin, yin, brows, gs)

    mark("barrel")
    # Pier and abutment sides facing into the openings (battered planes), courses as on the faces.
    prow = course_rows(-BURY, S, face_top, False, mi)
    for xc in X_C:
        for sgn in (1, -1):  # +1: the opening's +x side (faces -x)
            def ps(a, z, xc=xc, sgn=sgn):
                return (xc + sgn * hw(z, S), a, z)
            reg = Region(ps, (-sgn, 0.0, 0.0), sgn > 0, mi)
            rect_region(b, isl, reg, -W, W, prow)

    mark("piers")
    # The string course: front (a band of stones), top and underside, wrapping every PW along x.
    if d["string"]:
        for side in (1, -1):
            yo = side * (W + SC_OUT)
            front = Region(lambda a, z, yo=yo: (a, yo, z), (0.0, side, 0.0), side > 0, 1.0)
            ch = tp.CH
            rect_region(b, isl, front, -X_END, X_END,
                        [(hh["SC_LO"], hh["SC_HI"], "band_top", hh["SC_HI"] - ch)])
            for z, up in ((hh["SC_HI"], 1.0), (hh["SC_LO"], -1.0))[:2 if d["under"] else 1]:
                def hz(a, q, z=z, side=side):
                    return (a, side * (W + q), z)
                reg = Region(hz, (0.0, 0.0, up), False, 1.0)
                rect_region(b, isl, reg, -X_END, X_END, [(0.0, SC_OUT, "panel", -0.5)])

    mark("string")
    # Parapets: the outer face over the string course (on the face wall's plane), the inner face
    # from under the ballast, each with the rain-streaked band under the coping.
    for side in (1, -1):
        z_out = hh["SC_HI"] if d["string"] else hh["SC_LO"]
        outer = Region(lambda a, z, side=side: (a, side * W, z), (0.0, side, 0.0), side > 0, sc)
        rect_region(b, isl, outer, -X_END, X_END, course_rows(z_out, hh["PAR_TOP"], hh["PAR_TOP"], sc == 1.0, sc))
        inner = Region(lambda a, z, side=side: (a, side * PAR_IN, z), (0.0, -side, 0.0), side < 0, mi)
        rect_region(b, isl, inner, -X_END, X_END, course_rows(hh["DECK"], hh["PAR_TOP"], hh["PAR_TOP"], mi == 1.0, mi))

    mark("parapets")
    # The coping: cap-course segments on the portal's coping islands (top, both fronts, both undersides).
    ncop = d["cop"]
    lc = 2 * X_END / ncop
    k = isl["cop_ctop"].w / lc
    z0, z1 = hh["PAR_TOP"], hh["PAR_TOP"] + COP_H
    for side in (1, -1):
        y_in, y_out = side * COP_IN, side * COP_OUT
        for i in range(ncop):
            xa, xb = -X_END + i * lc, -X_END + (i + 1) * lc

            def u(x, xa=xa):
                return (x - xa) * k
            top = [(xa, y_out), (xb, y_out), (xb, y_in), (xa, y_in)]
            b.face([(x, y, z1) for x, y in top],
                   [isl["cop_ctop"].uv(u(x), COP_OUT - abs(y)) for x, y in top], "stone", (0.0, 0.0, 1.0))
            for yy, out in ((y_out, side), (y_in, -side)):
                fr = [(xa, z0), (xb, z0), (xb, z1), (xa, z1)]
                b.face([(x, yy, z) for x, z in fr], [isl["cop_cfront"].uv(u(x), z - z0) for x, z in fr], "stone",
                       (0.0, float(out), 0.0))
            if d["under"]:
                for ya, yb in ((side * W, y_out), (side * PAR_IN, y_in)):
                    q = [(xa, ya), (xb, ya), (xb, yb), (xa, yb)]
                    b.face([(x, y, z0) for x, y in q], [isl["cop_cunder"].uv(u(x), abs(y - ya)) for x, y in q], "stone",
                           (0.0, 0.0, -1.0))

    mark("coping")
    # The ends, 2 mm inside the lane borders: the deck's cross-section under the ballast, the
    # parapets, the string course and the coping (seen only on a viaduct standing alone).
    es = 2.5 if lod < 2 else 5.0  # the ends meet the neighbour's cliff face: coarse cells
    end_rows = course_rows(-BURY, hh["DECK"], face_top, False, es)
    for xs in (1, -1):
        x = xs * X_END

        def ep(a, z, x=x):
            return (x, a, z)
        reg = Region(ep, (float(xs), 0.0, 0.0), xs < 0, es)
        rect_region(b, isl, reg, -W, W, end_rows)
        for side in (1, -1):
            lo, hi = sorted((side * PAR_IN, side * W))
            rect_region(b, isl, reg, lo, hi, course_rows(hh["DECK"], hh["PAR_TOP"], hh["PAR_TOP"], False, reg.s))
            lo, hi = sorted((side * COP_IN, side * COP_OUT))
            b.face([(x, lo, z0), (x, hi, z0), (x, hi, z1), (x, lo, z1)],
                   [isl["cop_end"].uv(0.0, 0.0), isl["cop_end"].uv(tp.CAP_OUT, 0.0), isl["cop_end"].uv(tp.CAP_OUT, COP_H),
                    isl["cop_end"].uv(0.0, COP_H)], "stone", (float(xs), 0.0, 0.0))
            if d["string"]:
                lo, hi = sorted((side * W, side * (W + SC_OUT)))
                b.face([(x, lo, hh["SC_LO"]), (x, hi, hh["SC_LO"]), (x, hi, hh["SC_HI"]), (x, lo, hh["SC_HI"])],
                       [isl["cop_end"].uv(0.0, 0.0), isl["cop_end"].uv(SC_OUT, 0.0),
                        isl["cop_end"].uv(SC_OUT, SC_HI - SC_LO), isl["cop_end"].uv(0.0, SC_HI - SC_LO)],
                       "stone", (float(xs), 0.0, 0.0))
    mark("ends")
    return b.obj(name, [mats["stone"]])


# ---------------------------------------------------------------------------------------------
# Track
def build_track(name, n, lod, isl, mats):
    """rail_straight_center's track at this LOD, checked at ground height, then lifted to the deck."""
    d = LOD[lod]
    sl = d["sleepers"]
    m = _rail.track(_rail.straight_frames(d["track"]), _rail.straight_sleepers() if sl else None,
                    top_only=sl == "top")
    o = m.obj(name, {k: mats[v] for k, v in TRACK_SLOTS.items()})
    errs = _rail.border_section_errors(o, (0, 3)) + _rail.interior_errors(o, (0, 3))
    assert not errs, "\n".join(errs)
    h = n * STEP
    ti = isl["track"]
    me = o.data
    layer = me.uv_layers.new(name="UVMap")
    for loop in me.loops:
        co = me.vertices[loop.vertex_index].co
        lift = LIFT * co.z
        layer.data[loop.index].uv = ti.uv(abs(co.x) + lift, co.y + TRACK_T + lift)
    for v in me.vertices:
        v.co.z += h
    me.update()
    me.shade_smooth()
    me.set_sharp_from_angle(angle=math.radians(30.0))
    return o


def deck_section_errors(o, n):
    """The lifted track's section at both lane borders is `_rail.section()` raised by n x LEVEL_STEP,
    and its rail top stands at n x LEVEL_STEP + RAIL_TOP."""
    h = n * STEP
    want = [(t, z + round(h * 1e4)) for t, z in _rail.section()]
    errs = []
    for k in (0, 3):
        got = _rail.border_section(o, k)
        if got != want:
            errs.append(f"{o.name} edge {k}: lifted section {got[:4]}... != {want[:4]}...")
    top = max((o.matrix_world @ v.co).z for v in o.data.vertices)
    if abs(top - (h + _rail.RAIL_TOP)) > 1e-4:
        errs.append(f"{o.name}: rail top {top} != {h + _rail.RAIL_TOP}")
    return errs


def track_masks(msk, isl):
    """The track island's masks: rail_straight_center's analytic field (R snow 1, G porosity, A crib AO)
    sampled at (|x|, t) on its 16 px/m planar grid."""
    wet, ao = _rail.masks_field("straight")
    size = _rail.MASK_SIZE
    wet, ao = wet.reshape(size, size), ao.reshape(size, size)
    ti = isl["track"]
    x0, y0, x1, y1 = ti.px_box()
    sx, sy = ti.pw / ti.w, ti.ph / ti.h
    xs = (np.arange(x0, x1) + 0.5 - (ti.x0 + tp.GUT)) / sx
    ts = (np.arange(y0, y1) + 0.5 - (ti.y0 + tp.GUT)) / sy - TRACK_T
    k = size / (2 * hexgeo.HEX_R)
    cols = np.clip(((xs + hexgeo.HEX_R) * k).astype(int), 0, size - 1)
    rows = np.clip(((ts + hexgeo.HEX_R) * k).astype(int), 0, size - 1)
    msk[y0:y1, x0:x1, 0] = 1.0
    msk[y0:y1, x0:x1, 1] = wet[np.ix_(rows, cols)]
    msk[y0:y1, x0:x1, 2] = 0.0
    msk[y0:y1, x0:x1, 3] = ao[np.ix_(rows, cols)]


# ---------------------------------------------------------------------------------------------
def islands():
    """The portal's islands with its fern and tuft cards shrunk to stubs (the viaduct draws neither;
    `compose_maps` still fills them), plus the track's masks island."""
    isl = [i for i in tp._islands() if not i.name.startswith(("fern", "tuft"))]
    isl += [tp.Island(f"fern{k}", tp.FERN_L, tp.FERN_W, False, (16, 8)) for k in range(tp.FERN_VARIANTS)]
    isl += [tp.Island(f"tuft{k}", tp.TUFT, tp.TUFT, False, (8, 8)) for k in range(tp.TUFT_VARIANTS)]
    isl.append(tp.Island("track", APOTHEM + 0.1, 2 * TRACK_T, False,
                         (int(round((APOTHEM + 0.1) * TRACK_PX_PER_M)), int(round(2 * TRACK_T * TRACK_PX_PER_M)))))
    return tp.pack(isl)


def shared_bake(piece, isl, out):
    """The portal helper's templates baked onto its receiver (albedo through EMIT, normal, roughness,
    AO), as `_tunnel_portal.build_piece` bakes them; cached by receiver signature so the three
    heights share one bake. Returns (signature, baked arrays or None, stats)."""
    highs, receiver = tp.build_templates(isl, tp.stone_materials(BAKE_SEED))
    signature = bake.receiver_signature([receiver])
    stats, baked = {}, None
    if os.environ.get("HEXW_SKIP_BAKE") != "1":
        cache = BAKE_CACHE / f"{signature[:16]}.npz"
        if cache.is_file():
            data = np.load(cache)
            assert str(data["signature"]) == signature, "bake cache signature differs"
            px = {k: data[k] for k in ("albedo", "normal", "ao", "rough")}
            stats["shared"] = True
            print(f"{piece}: shared bake {signature[:16]} reused")
        else:
            scn = bpy.context.scene
            scn.world = scn.world or bpy.data.worlds.new(piece)
            scn.world.light_settings.distance = tp.AO_DIST
            bake.setup_cycles(samples=tp.SAMPLES)
            kw = dict(cage_extrusion=tp.CAGE, max_ray=tp.MAX_RAY, margin=tp.MARGIN)
            imgs = {k: bake.new_image(f"{piece}.{k}", tp.ATLAS, non_color=k != "albedo")
                    for k in ("albedo", "normal", "ao", "rough")}
            bake.bake_emit_input(highs, receiver, "Base Color", imgs["albedo"], **kw)
            bake.bake_to(receiver, "NORMAL", imgs["normal"], highs=highs, **kw)
            bake.bake_to(receiver, "ROUGHNESS", imgs["rough"], highs=highs, **kw)
            bake.bake_to(receiver, "AO", imgs["ao"], highs=highs, **kw)
            px = {}
            for k, im in imgs.items():
                p = bake.save_png(im, str(out / f"{piece}.{k}.tmp.png"))
                px[k] = bake.load_png(p)[1].reshape(tp.ATLAS, tp.ATLAS, 4)
                os.remove(p)
            BAKE_CACHE.mkdir(parents=True, exist_ok=True)
            np.savez_compressed(cache, signature=np.array(signature), **px)
            stats["shared"] = False
        cover = np.zeros((tp.ATLAS, tp.ATLAS), bool)
        for name in isl:
            if name.split("_")[0] in ("panel", "band", "v0", "v1", "key", "jamb", "cop"):
                x0, y0, x1, y1 = isl[name].px_box()
                cover[y0:y1, x0:x1] = True
        px["cover"] = cover
        baked = px
        stats["normalMissFraction"] = float((px["normal"][cover, 2] < 0.05).mean())
        a = px["ao"][cover, 0]
        stats["ao"] = {"mean": float(a.mean()), "p05": float(np.percentile(a, 5))}
        stats["albedoLuma"] = float(px["albedo"][cover, :3].mean())
        stats["seam"] = {f"{s}.{k}": tp.seam_errors(px[k], isl, tp.ATLAS, s) for s in ("panel", "band_top", "band_foot")
                         for k in ("albedo", "normal", "ao")}
        bad = {k: v for k, v in stats["seam"].items() if max(v.values()) > (0.06 if k.endswith("ao") else 0.02)}
        assert not bad, f"a stone strip island does not wrap: {bad}"
    for o in list(highs) + [receiver]:
        bpy.data.objects.remove(o, do_unlink=True)
    bpy.context.view_layer.update()
    return signature, baked, stats


def pier_boxes(n):
    """The piers and both abutments at their battered foot, ground to deck, in Blender
    (x0, x1, y0, y1, z0, z1): the walker's obstacles and the collider's legs."""
    s, top = heights(n)["S"], heights(n)["DECK"]
    foot = hw(0.0, s)
    out = [(-X_END, X_C[0] - foot, -W, W, -BURY, top)]
    for xa, xb in zip(X_C, X_C[1:]):
        out.append((xa + foot, xb - foot, -W, W, -BURY, top))
    out.append((X_C[-1] + foot, X_END, -W, W, -BURY, top))
    return out


def collider_obj(name, n):
    """The piers' boxes and the deck's slab over the crowns (coping included)."""
    hh = heights(n)
    boxes = pier_boxes(n) + [(-X_END, X_END, -COP_OUT, COP_OUT, hh["S"] + R_ARCH, hh["PAR_TOP"] + COP_H)]
    bm = bmesh.new()
    for x0, x1, y0, y1, z0, z1 in boxes:
        bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.LocRotScale(
            Vector(((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2)), None, Vector((x1 - x0, y1 - y0, z1 - z0))))
    me = bpy.data.meshes.new(name + "_src")
    bm.to_mesh(me)
    bm.free()
    src = scene.link(bpy.data.objects.new(name + "_src", me))
    col = collider.collider_from(src, "cap", name)
    bpy.data.objects.remove(src, do_unlink=True)
    return col


def build_piece(n, generator):
    piece = f"rail_viaduct_l{n}"
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    out.mkdir(parents=True, exist_ok=True)
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    isl = islands()
    signature, baked, stats = shared_bake(piece, isl, out)
    if stats:
        print(f"{piece}: bake stats {json.dumps(stats)}")
    alb, nrm, orm, msk = tp.compose_maps(isl, baked, {"piece": piece})
    track_masks(msk, isl)
    bake.write_png(tp.ATLAS, alb.reshape(-1, 4), str(out / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(tp.NORMAL_SIZE, tp._down(nrm, tp.ATLAS, tp.NORMAL_SIZE, renormal=True), str(out / f"{piece}.normal.png"))
    bake.write_png(tp.ORM_SIZE, tp._down(orm, tp.ATLAS, tp.ORM_SIZE), str(out / f"{piece}.orm.png"))
    bake.write_png(tp.MASK_SIZE, tp._down(msk, tp.ATLAS, tp.MASK_SIZE), str(out / f"{piece}.masks.png"))

    mats = {"stone": materials.pbr_material(f"{piece}_stone", (0.30, 0.29, 0.27, 1.0), 0.9),
            "viaduct_ballast": materials.pbr_material("viaduct_ballast", (0.205, 0.2, 0.21, 1.0), 0.95),
            "viaduct_sleeper": materials.pbr_material("viaduct_sleeper", (*_rail.SLEEPER_COLOR, 1.0), _rail.SLEEPER_ROUGH),
            "viaduct_steel": materials.pbr_material("viaduct_steel", (*_rail.STEEL_COLOR, 1.0), _rail.STEEL_ROUGH),
            "viaduct_head": materials.pbr_material("viaduct_head", (*_rail.HEAD_COLOR, 1.0), _rail.HEAD_ROUGH)}
    root = scene.empty(piece)
    lods, errs, density = [], [], None
    for li in range(3):
        stone = build_stone(f"{piece}_lod{li}_stone", n, li, isl, mats)
        if li == 0:
            density = tp.slot_density(stone, 0)
        track = build_track(f"{piece}_lod{li}_track", n, li, isl, mats)
        errs += deck_section_errors(track, n)
        for v in stone.data.vertices:
            if hexgeo.dist_to_border(v.co.x, v.co.y) < 1.5e-3:
                errs.append(f"{stone.name}: stone vertex {tuple(v.co)} on a border")
                break
        body = geom.join([stone, track], f"{piece}_lod{li}")
        body.parent = root
        lods.append(body)
    tris = [geom.tri_count(o) for o in lods]
    print(f"{piece}: tris per LOD {tris}, stone density {density}, stone parts {json.dumps(PARTS)}")
    assert not errs, "\n".join(errs)
    budgets.check(tris, "center", piece)
    assert density["median"] >= 256.0, density
    slots = [[m.name for m in o.data.materials] for o in lods]
    assert all(set(s) == set(slots[0]) for s in slots), slots

    col = collider_obj(f"{piece}_collider", n)
    col.parent = root
    for i, k in enumerate((0, 3)):
        c = hexgeo.lane_frame(k, 0.0, _rail.LANE_WIDTH)["center"]
        export.add_socket(f"socket-lane-{i}", (*c, 0.0), root, kind="rail", edge=k, offset=0.0,
                          width=_rail.LANE_WIDTH, deck=n)
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    rep_path = out / "export-report.json"
    rep = json.loads(rep_path.read_text())
    rep["pngFallbackSize"] = {"albedo": tp.PNG_ALBEDO}
    rep_path.write_text(json.dumps(rep, indent=2, sort_keys=True) + "\n")
    hh = heights(n)
    runtime_piers = [[[round(x0, 3), round(-y1, 3)], [round(x1, 3), round(-y1, 3)], [round(x1, 3), round(-y0, 3)],
                      [round(x0, 3), round(-y0, 3)]] for x0, x1, y0, y1, _z0, _z1 in pier_boxes(n)]
    bake.write_bake_report(
        str(out / "bake-report.json"), receiverSignature=signature,
        device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=tp.SAMPLES, marginPx=tp.MARGIN,
        cageExtrusionM=tp.CAGE, maxRayDistanceM=tp.MAX_RAY, aoDistanceM=tp.AO_DIST,
        channels={"albedo": {"size": tp.ATLAS, "colorspace": "srgb"},
                  "normal": {"size": tp.NORMAL_SIZE, "colorspace": "linear"},
                  "orm": {"size": tp.ORM_SIZE, "colorspace": "linear"},
                  "masks": {"size": tp.MASK_SIZE, "colorspace": "linear"}},
        texelDensityPxPerM={"stone": density}, bakeStats=stats, lodTris=tris,
        lod2Drops={"viaduct_sleeper": "sleepers 0.26 m wide: under a pixel at 400 m (0.31 m), as on "
                                      "rail_straight_center, whose LOD2 has none"},
        islands={k: [v.x0, v.y0, v.pw, v.ph] for k, v in sorted(isl.items())},
        viaduct={"deckLevel": n, "deckM": hh["H"], "railTopOverTileM": round(hh["H"] + _rail.RAIL_TOP, 4),
                 "ballastTopOverTileM": round(hh["H"] + _rail.BALLAST_TOP, 4), "springingM": round(hh["S"], 4),
                 "archRadiusM": R_ARCH, "archCentresX": list(X_C), "pierWidthM": PIER, "batter": BATTER,
                 "crownM": round(hh["S"] + R_ARCH, 4), "faceHalfWidthM": W, "parapetInnerM": PAR_IN,
                 "parapetTopOverTileM": round(hh["PAR_TOP"] + COP_H, 4), "stringCourseM": [hh["SC_LO"], hh["SC_HI"]],
                 "stoneEndX": X_END, "pierFootprintsRuntimeXZ": runtime_piers,
                 "sharedBake": "one bake for rail_viaduct_l1, _l2 and _l3 (same receiver signature)"},
        skipped=skip_bake)
    print(json.dumps({"piece": piece, "tris": tris, "density": density}))
    return root, lods
