"""Rail tunnel portal family: rail_tunnel_portal.py over the approved road portal helper _tunnel_portal.py
(owner-approved references assets-source/refs/rail_tunnel_portal ref-2 for the look, ref-3 for the layout,
2026-11-13; RL-Q1 O2). The approved helper is never edited: this file sets its module globals to the rail
dimensions at import time, re-derives the values it computed from the road numbers, and routes four of its
module-level names (`_islands`, `build_templates`, `jamb_stone`, `build_lod`) and its `export` through
wrappers, so `_tunnel_portal.build_piece` runs unchanged.

Section (m above the tile's level): a semicircular head of radius 2.6 springing at 2.4 (crown 5.0, about
5.2 m clear at the springing), the 0.45 m voussoir ring, the 0.12 m keystone and the 0.28 m coping put the
wall top at 5.85, about 1.85 m above the one-level (4 m) hill. The face stands 6 m inside edge 0
(X_F = APOTHEM - 6.0) and the wall is 13 m wide. Twelve courses of about 0.46 m (the road's 0.465 m) in
the road's rows: the foot band, two five-course panels and the top band. The jambs are four dressed stones
a side (ref-2's tall jambs) on the road's one-stone jamb islands.

The turf mound (RL-Q1 O2, ref-3): over the wall top behind the coping it heaps HEAP above the wall and from
X_1 eases back down to the cover level 4.0 at edge 0 with zero slope there, spanning edge 0's full 16 m at
4.0. Beside the wall it wraps the wall ends: rising from under the cutting's bank at the face to the wall
top at X_1, so the stone end returns show between the bank and the coping, and falling sideways to the
tile's field, which it meets exactly on the hex border off edge 0 (and dives under elsewhere). The bore's
lining ends at the back plane X_B, where the mound is still well above the crown.

Clearance: the generator imports the committed train GLBs (train_engine, train_carriage, train_carriage_b,
LOD0, as shipped) and checks every vehicle vertex against the intrados, and every portal vertex in front of
the back plane against the vehicles' envelope (`train_clearance`)."""
from __future__ import annotations

import hashlib
import json
import math
import pathlib
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Vector

import _tunnel_portal as tp
from lib import export, hexgeo

REPO = pathlib.Path(__file__).resolve().parents[3]
APOTHEM = hexgeo.APOTHEM

# ---------------------------------------------------------------------------------------------
# Rail dimensions on the road helper's globals.
tp.A = tp.B = 2.6  # a semicircular head
tp.J = 2.4
tp.RING, tp.KEY_UP = 0.45, 0.12
tp.CROWN = tp.J + tp.B
tp.FACE_TOP = tp.CROWN + tp.RING + tp.KEY_UP
tp.TOP = tp.FACE_TOP + tp.LOW_H + 0.16
tp.W = 6.5
tp.X_F = APOTHEM - 6.0
tp.X_B = tp.X_F + 3.6
assert tp.X_E == APOTHEM
# FaceMap binds its x default to X_F when the class is defined; rebind it to the rail face.
tp.FaceMap.__init__.__defaults__ = (tp.X_F,)
tp.CH = tp.FACE_TOP / 12
tp.COURSES = 5
tp.PH = tp.COURSES * tp.CH
# The wall strips wrap every 1.8 m along the wall (the road's 2.4 m): five-course panels at 2.4 m
# overflow the 1024² atlas (measured with the helper's own packer).
tp.PW = 1.8
PW_ = tp.PW
# The panel's vertical period on a whole number of pixels (599 px, TEXEL 258.10): seam_errors compares the
# top gutter with the bottom rows one rounded period apart, and at 258 the 0.22 px slip across the
# course joints failed the v wrap (normal 0.027 > 0.02, AO 0.064 > 0.06).
tp.TEXEL = round(tp.PH * tp.TEXEL) / tp.PH
tp._EXT = [tp.ring_pt(math.pi * i / 512, tp.RING) for i in range(513)]
# The keystone's half width at its top (it spans one of SEGS ring divisions of the semicircle).
SEGS = (11, 7, 5)
tp.KEY_HALF = (tp.A + tp.RING + tp.KEY_UP) * math.sin(math.pi / SEGS[0] / 2)
for lod_, segs in enumerate(SEGS):
    tp.LOD[lod_]["segs"] = segs
tp.LOD[0]["coping"], tp.LOD[1]["coping"] = 13, 6
# One strip per face cell near the ring: panel_cell below cuts each cell along the ring's own edge.
tp.LOD[0]["strip"] = tp.LOD[1]["strip"] = 100.0
tp.LOD[0]["fronds"] = 3
tp.LOD[1]["frond_segs"] = 1
tp.LAMP_S, tp.LAMP_Z0, tp.LAMP_Z1, tp.ARM_Z = 4.3, 2.95, 3.23, 3.51
# Fern clumps off the train's path (|s| >= 1.9 on the coping), the foot clumps beside the jambs.
tp.FERN_CLUMPS = [(-6.0, "coping"), (-5.35, "coping"), (-3.3, "coping"), (-2.6, "coping"), (1.9, "coping"),
                  (2.55, "coping"), (3.3, "coping"), (5.4, "coping"), (6.05, "coping"),
                  (-4.8, "foot"), (-3.9, "foot"), (3.9, "foot"), (4.7, "foot")]
tp.FRINGE_S = tp.W - 0.2

A, J, RING, W, TOP, X_F, X_B, X_E = tp.A, tp.J, tp.RING, tp.W, tp.TOP, tp.X_F, tp.X_B, tp.X_E
CROWN, FACE_TOP, CH, PH = tp.CROWN, tp.FACE_TOP, tp.CH, tp.PH
assert abs(tp.CROWN - 5.0) < 1e-9 and abs(TOP - 5.85) < 1e-9
assert abs(CH + 2 * PH + CH - FACE_TOP) < 1e-9, "the road's rows: foot band, two panels, top band"
assert all(s % 2 == 1 for s in SEGS), "the keystone sits at the middle division"
assert abs(tp.CROWN + tp.RING + tp.KEY_UP - tp.FACE_TOP) < 1e-9

# Jambs: N_JAMB stones a side on the road's jamb islands, each HJ tall (the islands, the jamb template and
# the template receiver are built with J set to HJ - JAMB_BURY, the LODs with the springing).
N_JAMB = 4
HJ = (J + tp.JAMB_BURY) / N_JAMB
J_STONE = HJ - tp.JAMB_BURY

# Mound: the wall top flat (heaping HEAP) from the coping's back to X_1, then easing to 4.0 at edge 0; the
# flank half width WM grows from W at X_1 to edge 0's 8 m; FLANK the sideways fall; DIVE the rim's depth
# under the field away from the hex border.
COVER = hexgeo.LEVEL_STEP
X_C = X_F + tp.CAP_BACK
X_1 = X_F + 2.6
HEAP, HEAP_EASE, FLANK, DIVE = 0.3, 1.5, 2.5, 0.25
LINING_COVER = 0.3  # the mound stays this far over the crown above the bore
EDGE0_HALF = hexgeo.HEX_R / 2
# Per LOD: mound columns between X_C and X_1 and from X_1 to edge 0, rows over the wall's width and on
# each flank; the back face's map scale.
MOUND = [{"top": 2, "fall": 4, "inner": 6, "flank": 3, "back": 1.0},
         {"top": 1, "fall": 2, "inner": 4, "flank": 2, "back": 1.0},
         {"top": 1, "fall": 2, "inner": 2, "flank": 1, "back": 2.5}]

TRAINS = ("train_engine", "train_carriage", "train_carriage_b")
RAIL_TOP = 0.46  # _rail.RAIL_TOP: the vehicles' z = 0
CLEAR_MIN = 0.15  # radial clearance the vehicles keep from the intrados and every portal vertex

_STATE = {"lod": 0, "report": {}}


# ---------------------------------------------------------------------------------------------
# The field and the mound
def hw(x):
    """The hexagon's half width at runtime x (x >= 0)."""
    return (APOTHEM - 0.5 * x) / math.sin(math.radians(60))


def d15(x, s):
    """Distance to the nearer of edges 1 and 5 (the border off edge 0), positive inside."""
    return APOTHEM - 0.5 * x - math.sin(math.radians(60)) * abs(s)


def field(x, s):
    return tp.cut_height_at(x, -s)


def s01(t):
    t = min(max(t, 0.0), 1.0)
    return t * t * (3.0 - 2.0 * t)


def wm(x):
    return W + (EDGE0_HALF - W) * s01((x - X_1) / (X_E - X_1))


def flank(x):
    return max(min(FLANK, hw(x) - wm(x)), 1e-3)


def crest(x, s):
    """The mound over the wall and the bore: the wall top heaped (the heap fading at the wall's ends),
    then easing to the cover level at edge 0 with zero slope."""
    lat = 1.0 - s01((abs(s) - (W - HEAP_EASE)) / HEAP_EASE)
    if x <= X_1:
        return TOP + HEAP * lat * s01((x - X_C) / (X_1 - X_C))
    return COVER + (TOP + HEAP * lat - COVER) * (1.0 - s01((x - X_1) / (X_E - X_1)))


def mound(x, s, inner):
    """Mound height at (x, s). `inner`: the turf over the wall top (|s| <= W behind the coping);
    otherwise the flank, which rises from under the bank at the face to the wall top at X_1."""
    if inner:
        return crest(x, s)
    a = abs(s)
    if a <= wm(x) and x >= X_1:
        return crest(x, s)
    f = field(x, s)
    under = f - DIVE * s01(d15(x, s) / 1.0)
    g = 1.0 - s01((a - wm(x)) / flank(x)) if a > wm(x) else 1.0
    lift = s01((x - X_F) / (X_1 - X_F)) * g
    return under + (crest(x, s) - under) * lift


def mound_normal(x, s, inner, h=1e-3):
    dx = (mound(x + h, s, inner) - mound(x - h, s, inner)) / (2 * h)
    ds = (mound(x, s + h, inner) - mound(x, s - h, inner)) / (2 * h)
    n = Vector((-dx, -ds, 1.0))
    n.normalize()
    return n


def mound_columns(d):
    xs = [X_F, X_C] + [X_C + (X_1 - X_C) * k / d["top"] for k in range(1, d["top"] + 1)]
    xs += [X_1 + (X_E - X_1) * k / d["fall"] for k in range(1, d["fall"] + 1)]
    return xs


def mound_rows(x, d):
    """Lateral positions at column x: the wall's width at fixed rows, the flanks stretched to the
    mound's edge there (the hex border near edge 0)."""
    edge = min(wm(x) + flank(x), hw(x)) if x < X_E - 1e-9 else EDGE0_HALF
    inner = [-W + 2 * W * k / d["inner"] for k in range(d["inner"] + 1)]
    out = [W + (edge - W) * k / d["flank"] for k in range(1, d["flank"] + 1)]
    return [-o for o in reversed(out)] + inner + out


def add_mound(b, d, isl):
    """The turf: a grid of cells over the columns; cells inside the wall's width ahead of the coping's
    back are the coping and wall top, so they are skipped. Returns the cells' (corner, inner) list."""
    pi_ = isl["park_turf"]
    xs = mound_columns(d)
    rows = [mound_rows(x, d) for x in xs]
    cells = 0
    for i in range(len(xs) - 1):
        for j in range(len(rows[i]) - 1):
            sa, sb = rows[i][j], rows[i][j + 1]
            sc, sd = rows[i + 1][j], rows[i + 1][j + 1]
            mid = (sa + sb + sc + sd) / 4
            inner = abs(mid) < W
            if inner and xs[i] < X_C - 1e-9:
                continue
            quad = [(xs[i], sa), (xs[i + 1], sc), (xs[i + 1], sd), (xs[i], sb)]
            pts, seen = [], []
            for x, s in quad:
                if any(abs(x - p[0]) < 1e-6 and abs(s - p[1]) < 1e-6 for p in seen):
                    continue
                seen.append((x, s))
                pts.append((x, s))
            if len(pts) < 3:
                continue
            co = [(x, s, mound(x, s, inner)) for x, s in pts]
            uvs = [pi_.uv((x - X_F) / 7.0, (s + 9.0) / 18.0) for x, s in pts]
            b.face(co, uvs, "turf", (0.0, 0.0, 1.0))
            cells += 1
    return cells


def end_returns(b, isl, lod):
    """The wall's stone end returns at |s| = W, from the face to X_1, from under the flank to the wall
    top; panel cells (u to the viewer's right), rows of PH."""
    sc = 1.0 if lod < 2 else 2.5
    low = min(mound(X_F + (X_1 - X_F) * k / 40, W + 1e-4, False) for k in range(41)) - 0.25
    for side in (1, -1):
        for r, h0, h1 in tp.panel_rows(TOP, sc, bottom=low):
            hc = r * PH * sc
            u0 = 0.0
            while u0 < X_1 - X_F - 1e-6:
                u1 = min(u0 + PW_ * sc, X_1 - X_F)

                def end_map(u, v, side=side, u0=u0, hc=hc):
                    x = (X_1 - (u + u0)) if side > 0 else (X_F + u + u0)
                    return (x, side * W, hc + v)
                tp.panel_cell(b, end_map, isl, 0.0, u1 - u0, h0 - hc, h1 - hc, scale=sc, expect=(0.0, side, 0.0))
                u0 = u1


def back_face(b, isl, sc):
    """The back face on edge 0 (seen from +x, the viewer's right is +s): |s| <= 8 from under the cutting
    to the cover level, where the mound meets edge 0. A cell over the bore's opening goes to the
    `stone_back` slot (hidden while the tunnel is complete), the rest stays stone."""
    for r, h0, h1 in tp.panel_rows(COVER, sc):
        hc = r * PH * sc
        s_c = -EDGE0_HALF
        while s_c < EDGE0_HALF - 1e-9:
            sb = min(s_c + PW_ * sc, EDGE0_HALF)

            def back_map(u, v, s_c=s_c, hc=hc):
                return (X_E, s_c + u, hc + v)
            slot = "stone_back" if cell_over_opening(s_c, sb, h0, h1) else "stone"
            tp.panel_cell(b, back_map, isl, 0.0, sb - s_c, h0 - hc, h1 - hc, slot=slot, scale=sc,
                          expect=(1.0, 0.0, 0.0))
            s_c = sb


# ---------------------------------------------------------------------------------------------
# Open tunnels (owner 2026-11-14, docs/superpowers/specs/2026-11-14-open-tunnels-design.md). The runtime
# hides a slot ending `_back` while the tunnel is complete and shows one ending `_open` only then, so a
# lone or incomplete portal draws exactly the approved piece. Complete, the barrel, the dark lining, the
# back plane and the back face's opening give way to a stone lining from the soffit to edge 0 on the
# panel island, under a turf ridge that rises out of the mound to RIDGE_CREST and carries on over every
# rail_tunnel_bore. The ring, the lining's mapping and the ridge's section are shared with
# _rail_tunnel_bore.py, so the two pieces meet vertex for vertex at edge 0 in every LOD.
OPEN_SLOTS = ("lining_back", "barrel_back", "stone_back", "stone_open", "turf_open")
tp.SLOTS = tp.SLOTS + OPEN_SLOTS
X_O = X_F + tp.SOFFIT  # the open lining's start: the ring stones' soffit's back edge
# Per LOD: the girth's UV period in panel heights, and the open lining's cells from X_O to edge 0.
OPEN_AROUND = (1, 1, 6)
OPEN_CELLS = (4, 2, 1)
# The ridge's section (s, h): RIDGE_CREST over the axis, easing with zero slope to the cover level at
# |s| = RIDGE_HALF, then diving RIDGE_DIVE under it by RIDGE_OUT (as the mound's rim dives), so it never
# lies coplanar on the cover meadow. The same samples in every LOD of both pieces.
RIDGE_CREST, RIDGE_HALF, RIDGE_OUT, RIDGE_DIVE = 5.75, 7.0, 7.4, 0.06
RIDGE_S = (0.0, 1.5, 3.0, 4.5, 6.0, 7.0, 7.4)


def cell_over_opening(s0, s1, h0, h1, n=12):
    """Whether the back-face cell [s0, s1] x [h0, h1] overlaps the bore's opening by more than 1 cm."""
    e = 0.01
    for i in range(n + 1):
        s = s0 + e + (s1 - s0 - 2 * e) * i / n
        for k in range(n + 1):
            h = h0 + e + (h1 - h0 - 2 * e) * k / n
            if abs(s) < A - e and -tp.BURY + e < h < J + math.sqrt(max(A * A - s * s, 0.0)) - e:
                return True
    return False


def open_ring(lod):
    """The open lining's ring for this LOD as (s, h, g), from the +s foot round to the -s foot: the
    portal lining's own profile (its LOD's arch divisions, so the lining meets the soffit's back edge
    vertex for vertex), the crown, and cuts on the profile's chords every OPEN_AROUND x PH of girth from
    the crown (the panel island's v period). g is the girth from the crown, the same on both sides, so
    a bore turned half round meets its neighbour on the same v."""
    d = tp.LOD[lod]
    prof = [(A, -tp.BURY)] + [tp.intrados(t) for t in tp.ellipse_divisions(d["segs"] * d["sub"])] + [(-A, -tp.BURY)]
    cum = [0.0]
    for (s0, h0), (s1, h1) in zip(prof, prof[1:]):
        cum.append(cum[-1] + math.hypot(s1 - s0, h1 - h0))
    half, period = cum[-1] / 2, PH * OPEN_AROUND[lod]
    marks = sorted({half} | {half + side * period * m for side in (1, -1) for m in range(1, int(half / period) + 1)})
    out = []
    for k, ((s0, h0), (s1, h1)) in enumerate(zip(prof, prof[1:])):
        out.append((s0, h0, cum[k]))
        for g in marks:
            if cum[k] + 1e-6 < g < cum[k + 1] - 1e-6:
                t = (g - cum[k]) / (cum[k + 1] - cum[k])
                out.append((s0 + (s1 - s0) * t, h0 + (h1 - h0) * t, g))
    out.append((*prof[-1], cum[-1]))
    return [(s if abs(s) > 1e-9 else 0.0, h, abs(g - half)) for s, h, g in out]


def open_section(lod):
    """The ring in 0.1 mm units, sorted (the end-plane contract both pieces assert)."""
    return sorted({(round(s * 1e4), round(h * 1e4)) for s, h, _ in open_ring(lod)})


def inward(s, h):
    """The lining's inward normal (Blender y, z) at a ring point: the jambs face across, the arch the axis."""
    if abs(s) >= A - 1e-6 and h <= J + 1e-6:
        return (-math.copysign(1.0, s), 0.0)
    n = Vector((-s, J - h)).normalized()
    return (n.x, n.y)


def open_lining(b, isl, lod, xs, slot, u_start=0.0):
    """The open lining between consecutive xs on open_ring(lod): every cell ends on the panel island's
    u wrap (u = PW, so each joint and both ends are on it) and spans the whole width from u = 0, except
    the first, which starts at u_start; v is the girth from the crown over OPEN_AROUND."""
    pi_, rg, around = isl["panel"], open_ring(lod), OPEN_AROUND[lod]
    period = PH * around
    for (sa, ha, ga), (sb, hb, gb) in zip(rg, rg[1:]):
        base = period * math.floor((ga + gb) / 2 / period)
        va, vb = (ga - base) / around, (gb - base) / around
        mid = inward((sa + sb) / 2, (ha + hb) / 2)
        for k, (xa, xb) in enumerate(zip(xs, xs[1:])):
            u0 = u_start if k == 0 else 0.0
            b.face([(xa, sa, ha), (xb, sa, ha), (xb, sb, hb), (xa, sb, hb)],
                   [pi_.uv(u0, va), pi_.uv(PW_, va), pi_.uv(PW_, vb), pi_.uv(u0, vb)], slot, (0.0, *mid))


def ridge(s):
    a = abs(s)
    if a <= RIDGE_HALF:
        return COVER + (RIDGE_CREST - COVER) * (1.0 - s01(a / RIDGE_HALF))
    return COVER - RIDGE_DIVE * s01((a - RIDGE_HALF) / (RIDGE_OUT - RIDGE_HALF))


def ridge_profile():
    ss = [-v for v in reversed(RIDGE_S[1:])] + list(RIDGE_S)
    return [(s, ridge(s)) for s in ss]


def ridge_normal(s, h=1e-3):
    n = Vector((0.0, -(ridge(s + h) - ridge(s - h)) / (2 * h), 1.0))
    n.normalize()
    return n


def ridge_section():
    return sorted((round(s * 1e4), round(h * 1e4)) for s, h in ridge_profile())


def add_ridge(b, isl, x0, x1, slot):
    """The ridge from x0 to x1, one cell along x (the turf set is sampled in world space; UV0 only
    addresses the masks, on the park_turf block)."""
    pt = isl["park_turf"]
    prof = ridge_profile()
    for (sa, ha), (sb, hb) in zip(prof, prof[1:]):
        b.face([(x0, sa, ha), (x1, sa, ha), (x1, sb, hb), (x0, sb, hb)],
               [pt.uv(0.2, 0.2), pt.uv(0.8, 0.2), pt.uv(0.8, 0.8), pt.uv(0.2, 0.8)], slot, (0.0, 0.0, 1.0))


def open_parts(b, isl, lod):
    """The portal's `_open` slots: the stone lining from X_O to edge 0 and the ridge from X_1 (buried in
    the mound there) to edge 0."""
    n, u_start = OPEN_CELLS[lod], 0.0
    if lod == 0:
        # LOD0 at true scale (the 256 px/m floor): whole panels back from edge 0, a part panel at X_O.
        xs = [X_O] + [X_E - PW_ * k for k in range(n - 1, -1, -1)]
        u_start = PW_ - (xs[1] - xs[0])
        assert 0.0 <= u_start < PW_, ("the open lining needs another LOD0 cell", u_start)
    else:
        xs = [X_O + (X_E - X_O) * k / n for k in range(n + 1)]
    xs[-1] = X_E
    open_lining(b, isl, lod, xs, "stone_open", u_start)
    buried = [(s, h, mound(X_1, s, abs(s) < W)) for s, h in ridge_profile()]
    low = [p for p in buried if p[1] > p[2] - 0.05]
    assert not low, ("the ridge is not buried in the mound at X_1", low)
    add_ridge(b, isl, X_1, X_E, "turf_open")


def end_plane(o, x, slot):
    """The (s, h) vertices of one slot on the plane x, in 0.1 mm units, sorted."""
    me, k = o.data, tp.SLOTS.index(slot) if isinstance(slot, str) else slot
    return sorted({(round(me.vertices[vi].co.y * 1e4), round(me.vertices[vi].co.z * 1e4))
                   for p in me.polygons if p.material_index == k for vi in p.vertices
                   if abs(me.vertices[vi].co.x - x) < 1e-4})


def edge0_errors(o, lod):
    """The portal's edge-0 contract with rail_tunnel_bore: the open lining ends on open_ring(lod), the
    ridge on ridge_profile()."""
    errs = []
    if end_plane(o, X_E, "stone_open") != open_section(lod):
        errs.append(f"lod{lod}: the open lining's edge-0 ring differs from open_section")
    if end_plane(o, X_E, "turf_open") != ridge_section():
        errs.append(f"lod{lod}: the ridge's edge-0 section differs from ridge_section")
    return errs


def split_back_slots(bm, isl):
    """The approved interior that the open lining replaces, onto `_back` slots: the dark lining, and the
    barrel (stone faces mapped on the barrel island)."""
    lining, stone = tp.SLOTS.index("lining"), tp.SLOTS.index("stone")
    lb, bb = tp.SLOTS.index("lining_back"), tp.SLOTS.index("barrel_back")
    uv = bm.loops.layers.uv["UVMap"]
    x0, y0, x1, y1 = isl["barrel"].px_box()
    n = {"lining": 0, "barrel": 0}
    for f in bm.faces:
        if f.material_index == lining:
            f.material_index = lb
            n["lining"] += 1
        elif f.material_index == stone:
            c = sum((l[uv].uv for l in f.loops), Vector((0.0, 0.0))) / len(f.loops)
            if x0 <= c.x * tp.ATLAS <= x1 and y0 <= c.y * tp.ATLAS <= y1:
                f.material_index = bb
                n["barrel"] += 1
    return n


# ---------------------------------------------------------------------------------------------
# Wrappers over the road helper
_orig_islands, _orig_templates, _orig_jamb, _orig_build_lod = tp._islands, tp.build_templates, tp.jamb_stone, tp.build_lod
_orig_panel_cell = tp.panel_cell


def _islands():
    """The road's islands, with the jamb islands one jamb stone tall."""
    tp.J = J_STONE
    try:
        return _orig_islands()
    finally:
        tp.J = J


def build_templates(isl, mats):
    """The road's templates and receiver, with the jamb template one jamb stone tall."""
    tp.J = J_STONE
    try:
        return _orig_templates(isl, mats)
    finally:
        tp.J = J


class _Shift:
    """A jamb map lifted by dv (the stone k of the stack)."""

    def __init__(self, P, dv):
        self.P, self.dv = P, dv
        for k in ("front", "up", "inner", "outer"):
            setattr(self, k, getattr(P, k))

    def __call__(self, u, v, dd):
        return self.P(u, v + self.dv, dd)


def jamb_stone(b, P, isl, mirror):
    """The template's jamb stone as the road builds it; in the LODs a stack of N_JAMB stones on the same
    islands, mirrored stone by stone."""
    if getattr(P, "o", None) is not None:
        return _orig_jamb(b, P, isl, mirror)
    tp.J = J_STONE
    try:
        for k in range(N_JAMB):
            _orig_jamb(b, _Shift(P, k * HJ), isl, mirror != (k % 2 == 1))
    finally:
        tp.J = J


def ring_top(lod):
    """The ring stones' top edges on the face for this LOD, each as (s, h) by descending s: the voussoir
    extrados vertices at r = RING (through the keystone's foot), the keystone's top at RING + KEY_UP;
    and the keystone's top corner |s| (the vertical step between the two)."""
    d = tp.LOD[lod]
    ths = tp.ellipse_divisions(d["segs"])
    nv = d["segs"]
    g = tp.RING_JOINT / 2 / math.hypot(tp.A, tp.B) * 2
    vous, key = [], []
    for i in range(nv):
        t0, t1 = ths[i] + (g if i else 0.0), ths[i + 1] - (g if i < nv - 1 else 0.0)
        ts = [t0 + (t1 - t0) * q / d["sub"] for q in range(d["sub"] + 1)]
        vous += [tp.ring_pt(t, RING) for t in ts]
        if i == nv // 2:
            key = [tp.ring_pt(t, RING + tp.KEY_UP) for t in ts]
    return sorted(vous, key=lambda p: -p[0]), sorted(key, key=lambda p: -p[0]), key[0][0]


def _top_at(poly, s):
    s = min(max(s, poly[-1][0]), poly[0][0])  # a cut at the polyline's end lands a rounding error outside
    for (s0, h0), (s1, h1) in zip(poly, poly[1:]):
        if s1 <= s <= s0:
            return h0 + (h1 - h0) * ((s - s0) / (s1 - s0) if s1 != s0 else 0.0)
    raise AssertionError(("no ring top at", s))


def panel_cell(b, P, isl, u0, u1, v0, v1, slot="stone", bottom=None, scale=1.0, expect=None, name="panel"):
    """The road's panel cell; a face cell cut by the ring (`bottom` given) instead follows the ring's own
    top edges exactly: the voussoirs', the keystone's, with vertical steps at the jambs' outer edges
    (|s| = A + RING) and the keystone's top corners. The road cuts along a chord per 0.4 m strip, which
    opens wedges beside a 2.4 m springing."""
    if bottom is None or not isinstance(P, tp.FaceMap):
        return _orig_panel_cell(b, P, isl, u0, u1, v0, v1, slot, bottom, scale, expect, name)
    vous, key, s_key = ring_top(_STATE["lod"])
    sc, hc = P.s_c, P.h_c
    steps = (A + RING, s_key, -s_key, -(A + RING))
    cuts = sorted({u0, u1, *[sc - s for s in steps if u0 < sc - s < u1]})
    for ua, ub in zip(cuts, cuts[1:]):
        sm = sc - (ua + ub) / 2
        if abs(sm) >= A + RING:
            _orig_panel_cell(b, P, isl, ua, ub, v0, v1, slot, None, scale, expect, name)
            continue
        poly = key if abs(sm) <= s_key else vous
        us = [ua] + sorted(sc - s for s, _ in poly if ua < sc - s < ub) + [ub]
        raw = [(u, _top_at(poly, sc - u) - hc) for u in us]
        bot = []
        for k, (u, v) in enumerate(raw):
            bot.append((u, max(v0, v)))
            if k + 1 < len(raw):
                un, vn = raw[k + 1]
                if min(v, vn) < v0 < max(v, vn):
                    bot.append((u + (un - u) * (v0 - v) / (vn - v), v0))
        out = []
        for k, (u, v) in enumerate(bot):
            if v <= v1:
                out.append((u, v))
            if k + 1 < len(bot):
                un, vn = bot[k + 1]
                if (v < v1) != (vn < v1) and min(v, vn) < v1 < max(v, vn):
                    out.append((u + (un - u) * (v1 - v) / (vn - v), v1))
        if len(out) < 2 or max(v1 - v for _, v in out) < 0.01:
            continue
        pts = out + [(out[-1][0], v1), (out[0][0], v1)]
        pts = [p for k, p in enumerate(pts) if p != pts[k - 1]]
        if len(pts) < 3:
            continue
        b.face([P(u, v) for u, v in pts], [isl[name].uv(u / scale, v / scale) for u, v in pts], slot, expect)


def crown_cap(b, isl):
    """LOD2's face over the keystone (|s| <= KEY_HALF): the road's LOD2 leaves it to a keystone that meets
    the coping, which this LOD's five-division keystone does not. Same cells and maps as the road's LOD2
    face rows."""
    sc = tp.LOD[2]["scale"]
    step = tp.PW * sc
    for ri, (r, h0, h1) in enumerate(tp.panel_rows(FACE_TOP, sc)):
        hc = r * PH * sc
        s_c = W + (0.83 * (ri + 1)) % tp.PW * sc
        while s_c - step > W:
            s_c -= step
        while s_c > -W:
            lo, hi = max(s_c - step, -W, -tp.KEY_HALF), min(s_c, W, tp.KEY_HALF)
            if hi > lo + 1e-6:
                panel_cell(b, tp.FaceMap(s_c, hc), isl, s_c - hi, s_c - lo, h0 - hc, h1 - hc, bottom=lambda u: 0.0,
                           scale=sc, expect=(-1.0, 0.0, 0.0), name="panel")
            s_c -= step


def strip_road_parts(bm, lod, isl):
    """Remove what the road layout puts where the rail one differs: the flat turf at the wall top, the
    6 m end faces, the back face at TOP; and at LOD2 the one-quad jamb fronts (re-added on the island)."""
    turf, stone = tp.SLOTS.index("turf"), tp.SLOTS.index("stone")
    gone = []
    for f in bm.faces:
        pts = [v.co for v in f.verts]
        xs, ys, zs = [p.x for p in pts], [p.y for p in pts], [p.z for p in pts]
        if f.material_index == turf:
            gone.append(f)
        elif f.material_index == stone:
            if all(abs(abs(y) - W) < 1e-6 for y in ys) and min(xs) >= X_F - 1e-6 and max(xs) > X_F + 0.01:
                gone.append(f)  # the road's end faces
            elif all(abs(x - X_E) < 1e-6 for x in xs):
                gone.append(f)  # the road's back face
            elif lod == 2 and all(abs(x - (X_F - tp.V_PROUD)) < 1e-6 for x in xs) and \
                    all(A - 1e-6 <= abs(y) <= A + RING + 1e-6 for y in ys) and max(zs) < J - tp.RING_JOINT / 2:
                gone.append(f)  # LOD2's jamb fronts
    bmesh.ops.delete(bm, geom=gone, context="FACES")
    return len(gone)


def lod2_jambs(b, isl):
    """LOD2's jamb front, one quad a side mapped onto one stone's island height."""
    top = J - tp.RING_JOINT
    k = (HJ - tp.RING_JOINT) / (top + tp.JAMB_BURY)
    for side in (1, -1):
        jm = tp.JambMap(side)
        q = [(0.0, 0.0), (RING, 0.0), (RING, top + tp.JAMB_BURY), (0.0, top + tp.JAMB_BURY)]
        m = (lambda u: RING - u) if side < 0 else (lambda u: u)
        b.face([jm(u, v, tp.V_PROUD) for u, v in q], [isl["jamb_front"].uv(m(u), v * k) for u, v in q], "stone",
               (-1.0, 0.0, 0.0))


def fix_back_uv(o, isl):
    """The back plane's UVs on its 32 px island for the 5 m crown (the road's divide by 4 m)."""
    bk, back = isl["back"], tp.SLOTS.index("back")
    me = o.data
    uvl = me.uv_layers["UVMap"].data
    for p in me.polygons:
        if p.material_index != back:
            continue
        for li in p.loop_indices:
            co = me.vertices[me.loops[li].vertex_index].co
            uvl[li].uv = bk.uv(0.5 - co.y / (2 * A + 1), (co.z + tp.BURY) / (CROWN + tp.BURY + 0.2))


def set_turf_normals(o):
    """Smooth turf with the mound's analytic normals (+Z on edge 0, as the cover meadow's ring)."""
    me = o.data
    turf, ridge_i, open_i = tp.SLOTS.index("turf"), tp.SLOTS.index("turf_open"), tp.SLOTS.index("stone_open")
    for p in me.polygons:
        p.use_smooth = p.material_index in (turf, ridge_i, open_i)
    normals = [None] * len(me.loops)
    for p in me.polygons:
        c = p.center
        inner = abs(c.y) < W and c.x > X_C - 1e-6
        for li in p.loop_indices:
            co = me.vertices[me.loops[li].vertex_index].co
            if p.material_index == ridge_i:
                normals[li] = tuple(ridge_normal(co.y))
                continue
            if p.material_index == open_i:
                normals[li] = (0.0, *inward(co.y, co.z))
                continue
            if p.material_index != turf:
                normals[li] = tuple(p.normal)
                continue
            n = Vector((0.0, 0.0, 1.0)) if abs(co.x - X_E) < 1e-6 else mound_normal(co.x, co.y, inner)
            normals[li] = tuple(n)
    me.normals_split_custom_set(normals)


def mound_errors(o, lod):
    """The mound's contract on this LOD's turf vertices."""
    me, turf = o.data, tp.SLOTS.index("turf")
    errs, edge0 = [], []
    for p in me.polygons:
        if p.material_index != turf:
            continue
        for vi in p.vertices:
            co = me.vertices[vi].co
            x, s, z = co.x, co.y, co.z
            if d15(x, s) < -1e-5 or x > X_E + 1e-6:
                errs.append(f"lod{lod}: turf vertex outside the hex at ({x:.3f}, {s:.3f})")
            if abs(x - X_E) < 1e-6:
                edge0.append(s)
                if abs(z - COVER) > 1e-5:
                    errs.append(f"lod{lod}: edge-0 turf at {z:.4f} m, not {COVER}")
            elif abs(d15(x, s)) < 1e-5 and abs(z - field(x, s)) > 1e-3:
                errs.append(f"lod{lod}: turf on the edge-1/5 border at {z:.4f}, the field {field(x, s):.4f}")
            if abs(s) <= A + 0.3 and X_F + tp.SOFFIT <= x <= X_B and z < CROWN + LINING_COVER - 1e-6:
                errs.append(f"lod{lod}: turf at {z:.3f} m over the bore at x {x:.3f}")
            if abs(s) <= W - 1e-6 and X_C - 1e-6 <= x <= X_1 and z < TOP - 1e-6:
                errs.append(f"lod{lod}: turf under the wall top at ({x:.3f}, {s:.3f})")
    if not edge0 or min(edge0) > -EDGE0_HALF + 1e-6 or max(edge0) < EDGE0_HALF - 1e-6:
        errs.append(f"lod{lod}: the turf does not span edge 0 (|s| <= 8): {min(edge0, default=None)}..{max(edge0, default=None)}")
    return errs[:12]


# ---------------------------------------------------------------------------------------------
# Train clearance
def train_vertices():
    """(name, sha256, (n, 2) array of (|y|, z above the tile)) for each vehicle's shipped LOD0."""
    out = []
    for name in TRAINS:
        path = REPO / "assets" / "kit" / "p1" / name / f"{name}.glb"
        bpy.ops.wm.read_factory_settings(use_empty=True)
        bpy.ops.import_scene.gltf(filepath=str(path))
        bpy.context.view_layer.update()
        o = bpy.data.objects[f"{name}_lod0"]
        pts = np.array([tuple(o.matrix_world @ v.co) for v in o.data.vertices], np.float64)
        out.append((name, hashlib.sha256(path.read_bytes()).hexdigest(),
                    np.stack([np.abs(pts[:, 1]), pts[:, 2] + RAIL_TOP], axis=1)))
    bpy.ops.wm.read_factory_settings(use_empty=True)
    return out


def intrados_clearance(yz):
    """Radial clearance of each (|y|, z) from the opening: the jambs' faces below the springing, the
    semicircle above it."""
    y, z = yz[:, 0], yz[:, 1]
    return np.where(z > J, A - np.hypot(y, z - J), A - y)


def envelope(trains):
    """The vehicles' upper envelope: the highest vehicle vertex at or outside each |y| (conservative)."""
    yz = np.concatenate([t[2] for t in trains])
    order = np.argsort(-yz[:, 0])
    ys, zs = yz[order, 0], np.maximum.accumulate(yz[order, 1])
    return ys[::-1], zs[::-1]


def portal_sweep(o, env):
    """Every portal vertex ahead of the back plane (the back plane is the tunnel's end, where trains have
    faded): its height over the vehicles' envelope at its |y|. Returns the smallest margin and where."""
    ys, zs = env
    back = tp.SLOTS.index("back")
    me = o.data
    worst = (math.inf, None)
    for p in me.polygons:
        if p.material_index == back:
            continue
        for vi in p.vertices:
            co = me.vertices[vi].co
            if co.x > X_B - 1e-6 or abs(co.y) > ys[-1]:
                continue
            k = np.searchsorted(ys, abs(co.y) - 0.02)
            h = zs[min(k, len(zs) - 1)]
            margin = co.z - h
            if margin < worst[0]:
                worst = (margin, (round(co.x, 3), round(co.y, 3), round(co.z, 3), tp.SLOTS[p.material_index]))
    return worst


def train_clearance(trains):
    rep = {}
    for name, sha, yz in trains:
        c = intrados_clearance(yz)
        i = int(np.argmin(c))
        top = yz[np.argmax(yz[:, 1])]
        rep[name] = {"sha256": sha, "vertices": int(len(yz)), "minRadialClearanceM": round(float(c[i]), 4),
                     "at": [round(float(yz[i, 0]), 3), round(float(yz[i, 1]), 3)],
                     "topM": round(float(top[1]), 3), "topClearanceM": round(float(CROWN - top[1]), 4),
                     "halfWidthM": round(float(yz[:, 0].max()), 3)}
    return rep


# ---------------------------------------------------------------------------------------------
def build_lod(spec, lod, isl, mats):
    _STATE["lod"] = lod
    o = _orig_build_lod(spec, lod, isl, mats)
    d = MOUND[lod]
    b = tp.Builder(tp.SLOTS)
    b.bm.free()
    b.bm = bmesh.new()
    b.bm.from_mesh(o.data)
    b.uv = b.bm.loops.layers.uv["UVMap"]
    removed = strip_road_parts(b.bm, lod, isl)
    split = split_back_slots(b.bm, isl)
    n0 = len(b.bm.faces)
    if lod == 2:
        lod2_jambs(b, isl)
        crown_cap(b, isl)
    cells = add_mound(b, d, isl)
    end_returns(b, isl, lod)
    back_face(b, isl, d["back"])
    open_parts(b, isl, lod)
    bmesh.ops.triangulate(b.bm, faces=[f for f in b.bm.faces if len(f.verts) > 3])
    b.bm.to_mesh(o.data)
    b.bm.free()
    fix_back_uv(o, isl)
    set_turf_normals(o)
    o.data.update()
    tally = {}
    for p in o.data.polygons:
        tally[tp.SLOTS[p.material_index]] = tally.get(tp.SLOTS[p.material_index], 0) + 1
    errs = mound_errors(o, lod)
    stone = tp.SLOTS.index("stone")
    behind = [tuple(round(c, 3) for c in p.center) for p in o.data.polygons
              if p.material_index == stone and p.normal.x < -0.99 and p.center.x > X_F + 1e-4]
    if behind:
        errs.append(f"lod{lod}: {len(behind)} face-plane stone faces behind the face x {X_F:.3f}, first {behind[0]}")
    sweep = portal_sweep(o, _STATE["envelope"])
    errs += edge0_errors(o, lod)
    print(f"{spec['piece']}: lod{lod} tris {len(o.data.polygons)} by slot {tally}; road parts removed {removed}, "
          f"kept {n0}, mound cells {cells}; split to _back {split}; portal sweep margin {sweep}")
    _STATE["report"].setdefault("lods", {})[f"lod{lod}"] = {"trisBySlot": tally, "moundCells": cells,
                                                           "splitToBack": split,
                                                           "portalSweepMarginM": round(sweep[0], 4),
                                                           "portalSweepAt": sweep[1]}
    assert not errs, errs
    assert sweep[0] >= CLEAR_MIN, ("a portal vertex sits in the train's path", sweep)
    return o


def _set_end_b():
    """The road's check_field asserts the wall's end lines stay END_B + 0.3 under the field."""
    ends = min(tp.cut_height_at(X_F - tp.CAP_OUT + (X_E - X_F + tp.CAP_OUT) * i / 40, z) for i in range(41)
               for z in (-W, W))
    tp.END_B = math.floor((ends - 0.35) * 20) / 20
    return ends


tp._islands = _islands
tp.build_templates = build_templates
tp.jamb_stone = jamb_stone
tp.build_lod = build_lod
tp.panel_cell = panel_cell


class _Export:
    """tp.export with export_glb forced to the deterministic single-thread run: the portal is a
    review surface for the open-tunnel pieces, and `_tunnel_portal.py` exports whatever its
    builder set, road or rail."""

    export_glb = staticmethod(lambda root, path, **_: export.export_glb(root, path, threads=1))
    write_export_report = staticmethod(export.write_export_report)


tp.export = _Export


def build_piece(spec, generator):
    trains = train_vertices()
    clear = train_clearance(trains)
    print(f"{spec['piece']}: train clearance {json.dumps(clear)}")
    worst = min(v["minRadialClearanceM"] for v in clear.values())
    assert worst >= CLEAR_MIN, ("a vehicle comes within CLEAR_MIN of the intrados", clear)
    _STATE["envelope"] = envelope(trains)
    _STATE["report"] = {"trains": clear}
    ends = _set_end_b()
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    root, lods = tp.build_piece(spec, generator)
    rep_path = out / "bake-report.json"
    rep = json.loads(rep_path.read_text())
    rep["lod2Drops"] = {f"{spec['piece']}_barrel_back": "LOD2 draws no barrel, as approved: its lining starts at "
                        "the soffit (the 0.75 m barrel is 2.4 px at 400 m, inside the arch's shadow)"}
    portal = rep["portal"]
    portal.update({
        "rail": True, "xMoundTop": round(X_1, 4), "xEdge0": round(X_E, 4), "wallHalfWidth": W, "faceTop": round(FACE_TOP, 4),
        "courseHeight": round(CH, 4), "jambStones": N_JAMB, "endLineFieldMinM": round(ends, 4), "endB": tp.END_B,
        "mound": {"heap": HEAP, "flank": FLANK, "dive": DIVE, "cover": COVER, "liningCover": LINING_COVER,
                  "crestAtBackPlane": round(crest(X_B, 0.0), 4)},
        "clearance": {"minM": CLEAR_MIN, "railTop": RAIL_TOP, "vehicles": _STATE["report"]["trains"],
                      "lods": _STATE["report"]["lods"],
                      "intradosAtHalfWidth": {str(y): round(J + math.sqrt(A * A - y * y), 4) for y in (0.0, 1.0, 1.35)}},
        "archNode": [round(X_B - 0.356, 1), 0.0],
        "open": {"slots": {"hiddenWhenComplete": [s for s in tp.SLOTS if s.endswith("_back") or s == "back"],
                           "shownWhenComplete": [s for s in tp.SLOTS if s.endswith("_open")]},
                 "liningFromX": round(X_O, 4), "liningToX": round(X_E, 4), "aroundPeriods": list(OPEN_AROUND),
                 "cells": list(OPEN_CELLS), "ridgeFromX": round(X_1, 4),
                 "ridge": {"crestM": RIDGE_CREST, "halfM": RIDGE_HALF, "outM": RIDGE_OUT, "diveM": RIDGE_DIVE,
                           "section": [[round(s, 4), round(h, 4)] for s, h in ridge_profile()]},
                 "ringPoints": [len(open_ring(k)) for k in range(3)]},
    })
    rep_path.write_text(json.dumps(rep, indent=2, sort_keys=True) + "\n")
    return root, lods
