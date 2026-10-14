"""River mouth bases (river_mouth_base, river_mouth_l_base): the whole bed of the straight-beach mouth.

Bed = `_mouth_profile.height(hand, x, y)` = min(coast profile, s-dependent channel) everywhere
(docs/kit/water-contract.md §3a; assets-source/refs/river_mouth/PLAN.md §3 "Mouth bed").

Border ring (identical in every LOD, compared on full x, y, z):
- ocean edges 0 and 1 at -4.0, beach edges 2 and 5 exactly `_beach.ring("straight")` (border_ring(16)
  plus EXTRA_S, coast heights), so beach seams meet beach_base vertex for vertex;
- the grass edge at 0 (lattice);
- the River edge exactly `_river.border_loop`'s lane-cut edge: lattice outside the cut, the bank tops,
  and the notch for the 1 m `river_edge` stub with the joint row at APOTHEM - 1 carrying
  `_river.profile_z` (river_edge's section). The mouth profile differs from it by at most 1.7 mm at
  the joint row's seaward-bank end (s = -7.72, where the coast term starts), which is not on the hex
  border.

Interior (one continuous cap per LOD, constrained Delaunay, every vertex on the profile):
- the coast rows of constant s (`_beach.ROWS_S`) outside the channel, cut where they cross the
  crease (channel = coast); the cut ends are crease vertices. Everything is parameterised along
  iso-s rows: on one row the coast is constant and the channel rises with d, so the in-channel set
  is one interval about the centre line and ends at s = 3 (cross-sections at constant x miss the
  region's inland side, which runs past the centre line's tip);
- the crease on both sides as constraint polylines where the coast is below land height (on land
  the bank top is C1 and needs none), with priority over every other constraint near it;
- the bed parabola's end d = w(s) (slope jump about 0.5) and lines at fixed d / w(s) (the
  river_edge joint offsets inland, snapped to the joint row) as constraint polylines: the profile
  depends only on s and d / w, so the long thin triangles between them follow the channel and need
  points only where s changes the profile;
- per-LOD seeds (row spacing, a lattice), points closer than NEAR to another point or constraint
  dropped (plan slivers give zero tangents), then deterministic refinement: the worst interior
  sample of every face over tolerance is inserted and the CDT rerun until every sample is inside.
Normals are per face side: the channel's gradient on channel faces, the coast's analytic normal on
coast faces, so the crease shades as one line. The one-row skirt (LOD2) fans each straight run of
a hex edge from one base vertex.
Tolerance (the same sampler refines and checks): 0.03 m where the profile is below -1.0 (wet,
swash) or the coast sets a dry bed (beach_base's strict reading); 0.10 m on the channel's dry banks
and on the coast's deep floor (s >= 7.36), upward only on the ocean floor (s >= 8).

Mirror ("l", River on edge 4): the right-handed construction's 2D inputs (interior points and
constraint segments) are reflected across the coast's seaward axis (Blender -30 degrees, which maps
edges 0<->1, 2<->5, 3<->4 and keeps s) before triangulating; heights, normals and slots come from
`_mouth_profile` with hand "l", and refinement reruns there. The ring is composed natively for
edge 4 and asserted equal, as a set, to the reflected right-handed ring.
"""
import json
import math
import os
import pathlib
import struct
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Vector
from mathutils.geometry import delaunay_2d_cdt

from lib import bake, collider, export, geom, hexgeo, materials, scene, uv

import _beach
import _mouth_profile
import _river

MP = _mouth_profile
WATER = MP.WATER
FLOOR = MP.FLOOR
WET_Z = -1.0
SHALLOW_S = _beach.SHALLOW_S
TOL_WET, TOL_DRY = 0.03, 0.10
REFINE = 0.9  # refine to this fraction of the tolerance
MASK_SIZE = 1024
SUB = hexgeo.BORDER_SUBDIV
A = hexgeo.APOTHEM
U = (MP.COS30, -MP.SIN30)  # the coast's seaward axis (Blender -30 degrees)
BARY_N = 6
BARY = [(i / BARY_N, j / BARY_N) for i in range(BARY_N + 1) for j in range(BARY_N + 1 - i)
        if not ((i == 0 and j == 0) or i == BARY_N or j == BARY_N)]
# PLAN budgets. LOD2 lands near 700 at the contract tolerance (the ring and skirt fix about 200, the
# fan's across-profile lines and the crease the rest), so 400 is reported, not asserted; the lane-cut
# 20% ceiling is asserted.
LOD2_MAX, LOD1_MAX, LOD0_MAX = 400, 1500, 3600
LOD2_RATIO = 0.20  # lane-cut ceiling (_river.LANE_CUT_LOD2_RATIO)

GRASS, SAND, WET, SEABED, SKIRT = _beach.SLOTS
BED = _river.BED
SLOTS = (GRASS, SAND, WET, SEABED, SKIRT, BED)

# Fan colour (owner, 2026-10-14: the bed's dark colour must blend into the sand). Per-face slots cannot
# blend, so COLOR_0 on the river_bed, beach_sand and beach_wet primitives multiplies the slot tint
# toward one target colour field, and its alpha is the detail weight (kit_material.wesl: 0 shows the
# set's mean colour with no relief and no live wet band). Linear means of the sets' albedo PNGs
# (assets/kit/p1/materials/<set>/; the shader reads the same mean from the KTX2 top mip) and the
# manifest tints: change either and rebuild.
GRAVEL_MEAN = np.array((0.2562, 0.2238, 0.1752))
PLASTER_MEAN = np.array((0.7027, 0.6453, 0.5389))
SAND_TINT = np.array((1.0, 0.95, 0.8))  # manifest beach_sand and beach_wet
SEABED_TINT = np.array((1.5, 1.46, 1.26))  # manifest beach_seabed
BED_TINT = np.array((0.78, 0.66, 0.5))  # manifest river_bed on every river piece
# The mouth's river_bed slot tint (manifest): gravel at the sand's mean colour, so COLOR_0.rgb is in
# [0, 1] for every target (gltf-validator) and inland it is BED_TINT / BED_SLOT_TINT (unchanged look).
BED_SLOT_TINT = np.array((2.743, 2.74, 2.461))
SAND_COL = PLASTER_MEAN * SAND_TINT
SEABED_COL = GRAVEL_MEAN * SEABED_TINT
BED_COL = GRAVEL_MEAN * BED_TINT
WB_G1 = 2.0  # depth under the coast (m) over which the fan's bed weight ramps from 0 to 1
WB_SEAM = 0.8  # fan faces above water with less bed weight are plaster (sand, wet), the rest gravel


def reflect(p):
    d = p[0] * U[0] + p[1] * U[1]
    return (2 * d * U[0] - p[0], 2 * d * U[1] - p[1])


def key(x, y):
    return (round(x, 5), round(y, 5))


# --- profile ---------------------------------------------------------------------------------------

def prof(hand, x, y):
    """(bed, channel, coast, s) at Blender (x, y)."""
    s = MP.seaward(x, y)
    co = MP.coast_height(s)
    ch = MP.channel(MP.dist(hand, x, y), s)
    return min(co, ch), ch, co, s


def gap(hand, x, y):
    """channel - coast: negative where the channel sets the bed."""
    _h, ch, co, _s = prof(hand, x, y)
    return ch - co


def np_height(hand, X, Y):
    """Vectorized (bed, channel, coast, s), the same formula as `_mouth_profile`."""
    def ss(a, b, x):
        t = np.clip((x - a) / (b - a), 0.0, 1.0)
        return t * t * (3.0 - 2.0 * t)
    S = X * MP.COS30 - Y * MP.SIN30
    t = S / MP.BEACH_TOP_S
    shelf = (-MP.SHELF_SLOPE * np.minimum(S, MP.SHELF_S)
             + (MP.FLOOR - WATER + MP.SHELF_SLOPE * MP.SHELF_S) * ss(MP.SHELF_S, MP.DROP_END_S, S))
    co = WATER + np.where(S <= MP.BEACH_TOP_S, MP.LAND, np.where(S < 0.0, MP.LAND * t * (2.0 - t), shelf))
    (ax, ay), (ux, uy) = MP.axis(hand)
    tt = np.maximum((X - ax) * ux + (Y - ay) * uy, 0.0)
    D = np.hypot(X - (ax + ux * tt), Y - (ay + uy * tt))
    w = MP.W_IN + (MP.W_SEA - MP.W_IN) * ss(MP.W_RAMP[0], MP.W_RAMP[1], S)
    dep = MP.D_IN + (MP.D_SHELF - MP.D_IN) * ss(MP.D_RAMP[0], MP.D_RAMP[1], S)
    bed = -(dep - 0.05) * np.maximum(0.0, 1.0 - (D / w) ** 2) - 0.05
    bank = ss(0.7 * w, w + 2.5, D)
    ch = WATER + np.minimum(MP.LAND, bed + (MP.LAND - bed) * bank)
    return np.minimum(co, ch), ch, co, S


def np_ss(a, b, x):
    t = np.clip((np.asarray(x, dtype=np.float64) - a) / (b - a), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def bed_weight(hand, X, Y):
    """(w, bed, s): w = 1 where the river bed's own colour shows, 0 on the coast and the crease. In the
    fan it ramps with the channel's depth under the coast (C1 at the crease); the ramp is 0.45 m at
    s = -8 so every bed point under GRASS_Z there is w = 1, and inland of s = -9 it is 1."""
    h, ch, co, s = np_height(hand, np.asarray(X, dtype=np.float64), np.asarray(Y, dtype=np.float64))
    g1 = 0.45 + (WB_G1 - 0.45) * np_ss(MP.BEACH_TOP_S, -4.0, s)
    w = np_ss(0.0, 1.0, np.maximum(co - ch, 0.0) / g1)
    return np.maximum(w, 1.0 - np_ss(-9.0, MP.BEACH_TOP_S, s)), h, s


def fan_color(hand, slot, X, Y, Z):
    """COLOR_0 (n, 4) of a cap primitive's vertices: rgb = target / slot mean (both slots' means are
    SAND_COL), a = detail weight. Target = the bed colour blended by w toward the coast's colour at that
    point: sand above the water, seabed under it (the coast's own slots there)."""
    w, _h, _s = bed_weight(hand, X, Y)
    under = np.asarray(Z) < WATER
    if slot == BED:
        ref = np.where(under[:, None], SEABED_COL, SAND_COL)
        a = np.where(under, 1.0, np_ss(WB_SEAM, 0.95, w))
    else:
        ref = np.broadcast_to(SAND_COL, (len(w), 3))
        a = 1.0 - np_ss(0.5, WB_SEAM, w)
    target = ref + (BED_COL - ref) * w[:, None]
    rgb = target / (GRAVEL_MEAN * BED_SLOT_TINT if slot == BED else SAND_COL)
    return np.concatenate([rgb, a[:, None]], axis=1)


# --- ring and domain -------------------------------------------------------------------------------

def lane_of(hand):
    return MP.RIVER_EDGE[hand]


def ring(hand):
    """The cap border loop [(x, y, z)], edge 0 first: _beach's straight ring on every edge but the
    River edge, which is _river's lane-cut edge (notch and joint row)."""
    lane = lane_of(hand)
    beach = _beach.ring("straight")
    sizes = [SUB + len(_beach.EXTRA_S) if k in _beach.LAYOUTS["straight"]["beach"] else SUB for k in range(6)]
    starts = [sum(sizes[:k]) for k in range(6)]
    assert sum(sizes) == len(beach), (sizes, len(beach))
    river = _river.border_loop((lane,))
    n_lane = len(river) - 5 * SUB
    out = []
    for k in range(6):
        if k == lane:
            block = river[lane * SUB:lane * SUB + n_lane]
        else:
            block = beach[starts[k]:starts[k] + sizes[k]]
            assert abs(math.hypot(block[0][0] - hexgeo.corner((k - 1) % 6)[0],
                                  block[0][1] - hexgeo.corner((k - 1) % 6)[1])) < 1e-9, (k, block[0])
        out += block
    for x, y, z in out:
        if abs(hexgeo.dist_to_border(x, y)) < 1e-6:
            h = prof(hand, x, y)[0]
            assert abs(z - h) < 1e-3, f"ring vertex ({x:.4f}, {y:.4f}) z {z:.5f} off the mouth bed {h:.5f}"
    return out


def joint_delta(hand):
    """Max |river_edge section - mouth profile| over the joint row (not on the hex border)."""
    loop = ring(hand)
    return max(abs(z - prof(hand, x, y)[0]) for x, y, z in loop if abs(hexgeo.dist_to_border(x, y)) > 1e-6)


def inside(hand, x, y, m=0.0):
    """Inside the cap domain (the hex minus the stub's notch) by at least m."""
    if hexgeo.dist_to_border(x, y) <= m:
        return False
    lx, ly = hexgeo.rotate_xy((x, y), -lane_of(hand))
    return not (lx > _river.JOINT_X - m and abs(ly) < _river.BANK_TOP + m)


# --- right-handed construction inputs --------------------------------------------------------------

ROW_DIR = (MP.SIN30, MP.COS30)  # along a row of constant s, from beach edge 2 toward beach edge 5


def row_at(s, tau):
    """The point on the iso-s row at signed distance tau from where it crosses the centre line."""
    return (s / MP.COS30 + tau * ROW_DIR[0], tau * ROW_DIR[1])


def crease_t(s, sigma):
    """Distance along the iso-s row from the centre line to where the channel stops setting the bed
    (channel = coast), on side sigma, or None when the centre is not in the channel. On one row the
    coast is constant and the channel rises with d, so the in-channel set is one interval."""
    if gap("r", *row_at(s, 0.0)) >= -1e-9:
        return None
    lo, hi, t = 0.0, None, 0.0
    while t < 12.0:
        t += 0.05
        if gap("r", *row_at(s, sigma * t)) >= -1e-9:
            hi = t
            break
        lo = t
    if hi is None:
        return None
    for _ in range(50):
        mid = 0.5 * (lo + hi)
        if gap("r", *row_at(s, sigma * mid)) < -1e-9:
            lo = mid
        else:
            hi = mid
    return hi


def crease_pt(s, sigma):
    if MP.coast_height(s) > -1e-3:
        return None  # on land the bank top is C1: no crease
    t = crease_t(s, sigma)
    if t is None:
        return None
    p = row_at(s, sigma * t)
    return p if inside("r", *p, 0.02) else None


def kink_pt(s, sigma):
    """The bed parabola's end d = w(s) on the iso-s row, where the channel sets the bed."""
    p = row_at(s, sigma * MP.width(s) / ROW_DIR[1])
    if gap("r", *p) > -LINE_GAP or not inside("r", *p, 0.02):
        return None
    return p


def polylines(fn, xs, tol, depth):
    """Polylines of fn over the parameter grid xs, bisected where the chord misses the curve by more
    than tol and at the ends of where fn exists."""
    lines, cur = [], []

    def refine(xa, pa, xb, pb, k):
        xm = 0.5 * (xa + xb)
        pm = fn(xm)
        if pm and k > 0 and math.dist(pm, ((pa[0] + pb[0]) / 2, (pa[1] + pb[1]) / 2)) > tol:
            refine(xa, pa, xm, pm, k - 1)
            cur.append(pm)
            refine(xm, pm, xb, pb, k - 1)

    def edge(xa, xb, exists_at_a):
        for _ in range(30):
            xm = 0.5 * (xa + xb)
            if (fn(xm) is not None) == exists_at_a:
                xa = xm
            else:
                xb = xm
        return xa

    prev_x, prev_p = None, None
    for x in xs:
        p = fn(x)
        if prev_x is not None:
            if prev_p and p:
                refine(prev_x, prev_p, x, p, depth)
            elif prev_p and not p:
                xe = edge(prev_x, x, True)
                pe = fn(xe)
                if pe and xe - prev_x > 1e-3:
                    refine(prev_x, prev_p, xe, pe, depth)
                    cur.append(pe)
                lines.append(cur)
                cur = []
            elif p and not prev_p:
                xe = edge(x, prev_x, True)
                pe = fn(xe)
                if pe and x - xe > 1e-3:
                    cur.append(pe)
                    refine(xe, pe, x, p, depth)
        if p:
            cur.append(p)
        prev_x, prev_p = x, p
    if cur:
        lines.append(cur)
    return [ln for ln in lines if len(ln) >= 2]


S_TIP = MP.SHELF_S  # the channel's centre meets the shelf here: the in-channel region ends at s = 3
Q_D = tuple(_river.OFFS[14 + i] for i in (0, 2, 4, 6, 8, 10, 12))  # 0 .. 5.85 m inland
NEAR = 0.03  # minimum plan distance between an interior point and another point or constraint
LINE_GAP = 0.02  # q-lines and the parabola-end lines stop where the channel is this far under the coast
S_START = -14.6  # below the lowest s inside the domain's channel (the joint row's y > 0 end)


def inputs_r(max_seg, spacing, step, tol, depth, qstep):
    """Right-handed interior points and constraint segments (Blender (x, y) pairs)."""
    pts, segs = [], []
    # Coast rows of constant s, cut where the channel sets the bed; the cut ends come from crease_t,
    # the same function as the crease polylines, so rows end exactly on crease vertices.
    for s in _beach.ROWS_S:
        pa, pb = _beach.edge_point("straight", 2, s), _beach.edge_point("straight", 5, s)
        c = row_at(s, 0.0)
        ta = (pa[0] - c[0]) * ROW_DIR[0] + (pa[1] - c[1]) * ROW_DIR[1]
        tb = (pb[0] - c[0]) * ROW_DIR[0] + (pb[1] - c[1]) * ROW_DIR[1]
        lo, hi = crease_t(s, -1), crease_t(s, 1)
        n = max(1, math.ceil((tb - ta) / max_seg))
        row = []
        for i in range(n + 1):
            tau = ta + (tb - ta) * i / n
            q = pa if i == 0 else pb if i == n else row_at(s, tau)
            if lo is not None and -lo <= tau <= hi:
                continue
            row.append((tau, q))
        if lo is not None:
            row += [(-lo, row_at(s, -lo)), (hi, row_at(s, hi))]
        row.sort()
        kept = [(t, q) for t, q in row if q in (pa, pb) or inside("r", *q, 0.02)]
        for (_ta, qa), (_tb, qb) in zip(kept, kept[1:]):
            mids = [(qa[0] + (qb[0] - qa[0]) * f, qa[1] + (qb[1] - qa[1]) * f) for f in (0.25, 0.5, 0.75)]
            if all(gap("r", *m) >= -1e-9 and inside("r", *m, 1e-4) for m in mids):
                segs.append((qa, qb))
        pts += [q for _t, q in kept]
    # Crease and parabola-end polylines over iso-s rows (the coast rows' s values included).
    grid = sorted({S_START + (S_TIP - S_START) * i / math.ceil((S_TIP - S_START) / step)
                   for i in range(math.ceil((S_TIP - S_START) / step) + 1)}
                  | {s for s in _beach.ROWS_S if S_START < s < S_TIP})
    tip = row_at(S_TIP, 0.0)
    crease = []
    for sigma in (1, -1):
        for is_crease, fn in ((True, lambda s: crease_pt(s, sigma)), (False, lambda s: kink_pt(s, sigma))):
            for ln in polylines(fn, grid, tol, depth):
                if math.dist(ln[-1], tip) < 0.6:
                    ln.append(tip)
                segs += list(zip(ln, ln[1:]))
                pts += ln
                if is_crease:
                    crease += ln
    # Lines along the channel at fixed d / w(s) (the river_edge joint offsets inland): the profile
    # depends only on s and d / w, so these hold the across-channel shape and need points only where
    # s changes the profile (the fan, s > -8).
    qgrid = sorted({S_START}
                   | {MP.BEACH_TOP_S + (S_TIP - MP.BEACH_TOP_S) * i / math.ceil((S_TIP - MP.BEACH_TOP_S) / qstep)
                      for i in range(math.ceil((S_TIP - MP.BEACH_TOP_S) / qstep) + 1)})
    joint = [(x, y) for x, y, _z in ring("r") if abs(x + _river.JOINT_X) < 1e-6]
    for d in Q_D:
        for sigma in ((1,) if d == 0.0 else (1, -1)):
            def fn(s, q=d / MP.W_IN, sg=sigma):
                pq = row_at(s, sg * q * MP.width(s) / ROW_DIR[1])
                return pq if gap("r", *pq) < -LINE_GAP and inside("r", *pq, 0.02) else None
            for ln in polylines(fn, qgrid, tol, depth):
                j = min(joint, key=lambda pj: math.dist(pj, ln[0]))
                if math.dist(j, ln[0]) < 0.06:
                    ln[0] = j
                segs += list(zip(ln, ln[1:]))
                pts += ln
    # Lattice seeds: the flats beyond the coast strip and the channel.
    if spacing:
        for x, y in hexgeo.lattice_points(spacing, spacing * 0.5):
            if not inside("r", x, y, spacing * 0.5):
                continue
            s = MP.seaward(x, y)
            g = gap("r", x, y)
            if g < -1e-6 or s < MP.BEACH_TOP_S - spacing * 0.5 or s > MP.DROP_END_S + spacing * 0.5:
                pts.append((x, y))
    return pts, segs, crease


# --- cap ---------------------------------------------------------------------------------------------

def classify(hand, x, y, z):
    """(class, |error| / tol, error) of a surface point against the profile."""
    h, ch, co, s = prof(hand, x, y)
    err = z - h
    coast_set = ch >= co - 1e-9
    if coast_set and s >= SHALLOW_S:
        if s >= MP.DROP_END_S and err < -1e-4:
            return "deep", 1e3, err
        return "deep", abs(err) / TOL_DRY, err
    if h < WET_Z:
        return "wet", abs(err) / TOL_WET, err
    if coast_set:
        return "coastDry", abs(err) / TOL_WET, err
    return "channelDry", abs(err) / TOL_DRY, err


def triangulate(hand, loop, interior, edges_xy):
    pts = [(x, y) for x, y, _ in loop]
    zs = [z for _, _, z in loop]
    index = {key(x, y): i for i, (x, y) in enumerate(pts)}
    nb = len(loop)

    def add(p):
        k = key(*p)
        if k not in index:
            index[k] = len(pts)
            pts.append(p)
            zs.append(prof(hand, *p)[0])
        return index[k]
    for p in interior:
        add(p)
    edges = sorted({tuple(sorted((add(a), add(b)))) for a, b in edges_xy if key(*a) != key(*b)})
    out_v, _e, out_f, orig_v, _oe, _of = delaunay_2d_cdt([Vector(p) for p in pts], edges, [list(range(nb))], 1, 1e-6)
    verts, steiner = [], 0
    for i, v in enumerate(out_v):
        if orig_v[i]:
            verts.append((v.x, v.y, zs[orig_v[i][0]]))
        else:
            assert inside(hand, v.x, v.y, 1e-6), f"CDT added a Steiner vertex on the ring at {tuple(v)}"
            verts.append((v.x, v.y, prof(hand, v.x, v.y)[0]))
            steiner += 1
    faces = []
    for f in out_f:
        a, b, c = (Vector(verts[i]) for i in f[:3])
        faces.append(list(f) if (b - a).cross(c - a).z > 0 else list(reversed(f)))
    thin = sum(1 for f in faces if abs((verts[f[1]][0] - verts[f[0]][0]) * (verts[f[2]][1] - verts[f[0]][1])
                                         - (verts[f[2]][0] - verts[f[0]][0]) * (verts[f[1]][1] - verts[f[0]][1])) < 1e-6)
    if os.environ.get("HEXW_MOUTH_DEBUG"):
        ar = sorted((abs((verts[f[1]][0] - verts[f[0]][0]) * (verts[f[2]][1] - verts[f[0]][1])
                         - (verts[f[2]][0] - verts[f[0]][0]) * (verts[f[1]][1] - verts[f[0]][1])) / 2,
                     tuple(round(verts[i][j], 3) for i in f for j in (0, 1))) for f in faces)[:3]
        print(f"  {hand}: {thin} faces under 5e-7 m2 in plan; smallest {ar}")
    return verts, faces, steiner


def face_errors(hand, verts, faces):
    """Per face: (worst ratio, worst insertable sample point); plus the max error per class."""
    worst = {"wet": 0.0, "coastDry": 0.0, "channelDry": 0.0, "deep": 0.0}
    per_face = []
    for f in faces:
        a, b, c = (verts[i] for i in f)
        fr, br, fp = 0.0, 0.0, None
        for u, v in BARY:
            x = a[0] + (b[0] - a[0]) * u + (c[0] - a[0]) * v
            y = a[1] + (b[1] - a[1]) * u + (c[1] - a[1]) * v
            z = a[2] + (b[2] - a[2]) * u + (c[2] - a[2]) * v
            cls, r, err = classify(hand, x, y, z)
            worst[cls] = max(worst[cls], abs(err))
            fr = max(fr, r)
            if r > br and u > 0 and v > 0 and u + v < 1 - 1e-9:  # interior samples only: no slivers
                br, fp = r, (x, y)
        per_face.append((fr, fp))
    return per_face, worst


def seg_dist(p, a, b):
    ax, ay = b[0] - a[0], b[1] - a[1]
    t = max(0.0, min(1.0, ((p[0] - a[0]) * ax + (p[1] - a[1]) * ay) / (ax * ax + ay * ay or 1.0)))
    return math.hypot(p[0] - a[0] - ax * t, p[1] - a[1] - ay * t)


def clean(hand, pts, segs, ring_keys, crease_keys):
    """Drop interior points outside the domain, closer than NEAR to an earlier point (segments are
    remapped to the kept one) or closer than NEAR to a constraint segment they do not end (with their
    own segments): such points make plan slivers, whose zero UV area gives zero tangents."""
    kept, where = [], {}
    for q in pts:
        k = key(*q)
        if k in where:
            continue
        if k not in ring_keys and not inside(hand, *q, 1e-4):
            where[k] = None
            continue
        twin = next((r for r in kept if abs(r[0] - q[0]) < NEAR and abs(r[1] - q[1]) < NEAR
                     and math.dist(r, q) < NEAR), None) if k not in ring_keys and k not in crease_keys else None
        where[k] = twin if twin is not None else q
        if twin is None:
            kept.append(q)
    segs = [(where.get(key(*a)), where.get(key(*b))) for a, b in segs]
    segs = [(a, b) for a, b in segs if a is not None and b is not None and key(*a) != key(*b)]
    drop, cut = set(), set()
    for q in kept:
        k = key(*q)
        if k in ring_keys:
            continue
        for a, b in segs:
            if key(*a) != k and key(*b) != k and seg_dist(q, a, b) < NEAR:
                if k in crease_keys:
                    cut.add((key(*a), key(*b)))  # the crease wins: drop the other constraint
                    continue
                drop.add(k)
                break
    pts = [q for q in kept if key(*q) not in drop]
    segs = [(a, b) for a, b in segs if key(*a) not in drop and key(*b) not in drop and (key(*a), key(*b)) not in cut]
    return pts, segs


def build_points(hand, params):
    """The refined 2D input of one LOD: (loop, interior points, constraint segments)."""
    pts, segs, crease = inputs_r(*params)
    if hand == "l":
        pts = [reflect(p) for p in pts]
        segs = [(reflect(a), reflect(b)) for a, b in segs]
        crease = [reflect(p) for p in crease]
    loop = ring(hand)
    if hand == "l":
        mine = {key(x, y) for x, y, _ in loop}
        mirrored = {key(*reflect((x, y))) for x, y, _ in ring("r")}
        assert mine == mirrored, f"mirror ring mismatch: {len(mine ^ mirrored)} points"
    pts, segs = clean(hand, pts, segs, {key(x, y) for x, y, _ in loop}, {key(*q) for q in crease})
    for it in range(60):
        verts, faces, steiner = triangulate(hand, loop, pts, segs)
        per_face, worst = face_errors(hand, verts, faces)
        new = [p for _k, p in sorted({key(*p): p for r, p in per_face if r > REFINE and p is not None}.items())]
        bad = [r for r, p in per_face if r > REFINE and p is None]
        assert not bad, f"{hand}: {len(bad)} faces over tolerance with no insertable sample"
        if not new:
            return loop, verts, faces, worst, it, steiner
        if os.environ.get("HEXW_MOUTH_DEBUG"):
            top = max(per_face, key=lambda t: t[0])
            print(f"  {hand} iter {it}: faces {len(faces)}, failing {len(new)}, worst {top[0]:.3f} at {top[1]}, {worst}")
        pts += new
    raise AssertionError(f"{hand}: refinement did not converge")


def vertex_normal(hand, x, y, z):
    lane = lane_of(hand)
    if abs(hexgeo.dist_to_border(x, y)) < 1e-6:
        k = _beach.edge_of(x, y)
        lay = _beach.LAYOUTS["straight"]
        if (k in lay["beach"] or k in lay["ocean"]) and k != lane:
            return _beach.normal("straight", x, y)
        return (0.0, 0.0, 1.0)
    lx, ly = hexgeo.rotate_xy((x, y), -lane)
    e = 1e-4
    if abs(lx - _river.JOINT_X) < 1e-6 and abs(ly) <= _river.BANK_TOP + 1e-6:
        g = (_river.profile_z(ly + e) - _river.profile_z(ly - e)) / (2 * e)
        gx, gy = hexgeo.rotate_xy((0.0, g), lane)
    else:
        gx = (prof(hand, x + e, y)[0] - prof(hand, x - e, y)[0]) / (2 * e)
        gy = (prof(hand, x, y + e)[0] - prof(hand, x, y - e)[0]) / (2 * e)
    n = Vector((-gx, -gy, 1.0))
    return tuple(n.normalized())


def loop_normal(hand, x, y, z, channel_side):
    """Ring and joint vertices as `vertex_normal`; elsewhere the gradient of the surface on the face's
    own side of the crease (channel or coast), so the crease shades as one sharp line."""
    lx, ly = hexgeo.rotate_xy((x, y), -lane_of(hand))
    if abs(hexgeo.dist_to_border(x, y)) < 1e-6 or (abs(lx - _river.JOINT_X) < 1e-6 and abs(ly) <= _river.BANK_TOP + 1e-6):
        return vertex_normal(hand, x, y, z)
    if not channel_side:
        return _beach.normal("straight", x, y)
    e = 1e-4

    def ch(px, py):
        return MP.channel(MP.dist(hand, px, py), MP.seaward(px, py))
    gx = (ch(x + e, y) - ch(x - e, y)) / (2 * e)
    gy = (ch(x, y + e) - ch(x, y - e)) / (2 * e)
    return tuple(Vector((-gx, -gy, 1.0)).normalized())


def slot_of(hand, x, y):
    """Inland the river's slots; in the fan (s > -8) gravel under the water and where the bed weight
    reaches WB_SEAM, and the coast's plaster slots by height on the banks between that seam and the
    crease, so the crease joins like slots (gravel to seabed, plaster to sand) and COLOR_0 blends."""
    h, ch, co, s = prof(hand, x, y)
    if ch < co - 1e-6:
        if s <= MP.BEACH_TOP_S:
            return GRASS if h >= _river.GRASS_Z else BED
        if h < WATER or float(bed_weight(hand, x, y)[0]) >= WB_SEAM:
            return BED
        return WET if h < _beach.WET_TOP_Y else SAND
    return _beach.slot_of("straight", x, y)


def cap(name, hand, params, mats):
    loop, verts, faces, worst, iters, steiner = build_points(hand, params)
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate(clean_customdata=False)
    mesh.update()
    obj = scene.link(bpy.data.objects.new(name, mesh))
    for nm in SLOTS:
        obj.data.materials.append(mats[nm])
    for p in obj.data.polygons:
        p.use_smooth = True
        p.material_index = SLOTS.index(slot_of(hand, p.center.x, p.center.y))
    side = [gap(hand, p.center.x, p.center.y) < -1e-9 for p in obj.data.polygons]
    normals = [None] * len(obj.data.loops)
    for p in obj.data.polygons:
        for li in p.loop_indices:
            v = obj.data.vertices[obj.data.loops[li].vertex_index].co
            normals[li] = loop_normal(hand, v.x, v.y, v.z, side[p.index])
    obj.data.normals_split_custom_set(normals)
    obj.data.update()
    return obj, loop, {"refineIterations": iters, "steinerVertices": steiner,
                       **{f"{k}MaxM": round(v, 4) for k, v in worst.items()}}


def skirt(name, hand, loop, rows, mats):
    """Soil walls under the hex-border ring points down to the tile base: the lane span between the
    bank tops is left open (the river_edge mouth wall), segments already at the base are skipped and
    a segment with one end there is one triangle."""
    lane = lane_of(hand)
    border = [p for p in loop if abs(hexgeo.dist_to_border(p[0], p[1])) < 1e-6]
    n = len(border)
    verts, faces, runs = [], [], []
    for i in range(n):
        a, b = border[i], border[(i + 1) % n]
        m = hexgeo.rotate_xy(((a[0] + b[0]) / 2, (a[1] + b[1]) / 2), -lane)
        if abs(m[0] - A) < 1e-4 and abs(m[1]) < _river.BANK_TOP - 1e-6:
            continue
        da, db = a[2] - FLOOR, b[2] - FLOOR
        if da < 1e-6 and db < 1e-6:
            continue
        base = len(verts)
        if rows == 1:
            runs.append((i, _beach.edge_of((a[0] + b[0]) / 2, (a[1] + b[1]) / 2)))
            continue
        if da < 1e-6 or db < 1e-6:
            top = b if da < 1e-6 else a
            verts += [(a[0], a[1], FLOOR), (b[0], b[1], FLOOR), top]
            faces.append([base, base + 1, base + 2])
            continue
        for r in range(rows + 1):
            t = r / rows
            verts += [(a[0], a[1], a[2] + (FLOOR - a[2]) * t), (b[0], b[1], b[2] + (FLOOR - b[2]) * t)]
        for r in range(rows):
            p = base + 2 * r
            faces += [[p, p + 2, p + 3], [p, p + 3, p + 1]]
    # One row: each straight run of segments on one hex edge is one wall, fanned from the base
    # vertex under its first ring point (half the triangles of a quad strip, same top edge).
    groups = []
    for i, k in runs:
        if groups and groups[-1][-1][0] == (i - 1) % n and groups[-1][-1][1] == k:
            groups[-1].append((i, k))
        else:
            groups.append([(i, k)])
    if len(groups) > 1 and groups[0][0][0] == 0 and groups[-1][-1][0] == n - 1 and groups[0][0][1] == groups[-1][-1][1]:
        groups[0] = groups.pop() + groups[0]
    for g in groups:
        tops = [border[g[0][0]]] + [border[(i + 1) % n] for i, _k in g]
        if tops[-1][2] > tops[0][2]:
            tops.reverse()  # fan from under the higher end: ring points on the drop can be collinear
            # with the ocean corner (s = 4, 4.5 and 8 on a beach edge), which made a zero-area fan
        base = len(verts)
        verts += [(x, y, z) for x, y, z in tops]
        # The apex is the ring point itself where it already sits on the base (no zero-area fans).
        if tops[0][2] - FLOOR > 1e-6:
            b0 = len(verts)
            verts.append((tops[0][0], tops[0][1], FLOOR))
        else:
            b0 = base
        for j in range(len(tops) - 1):
            if b0 != base + j and (tops[j][2] - FLOOR > 1e-6 or tops[j + 1][2] - FLOOR > 1e-6):
                faces.append([b0, base + j, base + j + 1])
        if tops[-1][2] - FLOOR > 1e-6:
            verts.append((tops[-1][0], tops[-1][1], FLOOR))
            faces.append([b0, base + len(tops) - 1, len(verts) - 1])
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate(clean_customdata=False)
    mesh.update()
    obj = scene.link(bpy.data.objects.new(name, mesh))
    for nm in SLOTS:
        obj.data.materials.append(mats[nm])
    for p in obj.data.polygons:
        if p.normal.x * p.center.x + p.normal.y * p.center.y < 0:
            p.flip()
        p.material_index = SLOTS.index(SKIRT)
        assert p.area > 1e-3, f"{name}: skirt face {p.index} at {tuple(p.center)} has area {p.area}"
    obj.data.update()
    obj.data.uv_layers.new(name="UVMap")
    for d in obj.data.uv_layers["UVMap"].data:
        d.uv = (0.005, 0.005)
    return obj


def loop_key(obj, loop):
    """The cap's loop vertices (x, y, z) to 0.1 mm, asserted to be exactly the ring."""
    want = {(round(x, 4), round(y, 4), round(z, 4)) for x, y, z in loop}
    have = {(round(v.co.x, 4), round(v.co.y, 4), round(v.co.z, 4)) for v in obj.data.vertices}
    missing = want - have
    assert not missing, f"{obj.name}: {len(missing)} ring vertices missing"
    return want


def check_beach_edges(loop):
    """Beach-edge ring points are _beach's straight ring points (EXTRA_S included), x, y, z."""
    beach = {(round(x, 6), round(y, 6), round(z, 6)) for x, y, z in _beach.ring("straight")
             if _beach.edge_of(x, y) in _beach.LAYOUTS["straight"]["beach"]
             and abs(hexgeo.dist_to_border(x, y)) < 1e-6}
    mine = {(round(x, 6), round(y, 6), round(z, 6)) for x, y, z in loop}
    assert beach <= mine, f"beach-edge ring differs from beach_base: {len(beach - mine)} points"


def masks(hand, out, piece, size=MASK_SIZE):
    """Analytic masks over the planar UV0: R snow 1 where the bed is above the water, 0 under it;
    G wetness 0.9 where the channel sets the bed (river bases), else beach_base's rule (0.7 dry, 1.0
    on the damp band, 0 under the sea); B night 0; A AO 1."""
    c = (np.arange(size, dtype=np.float64) + 0.5) / size * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    h, ch, co, s = np_height(hand, xs, ys)
    snow = np.where(h >= WATER, 1.0, 0.0).ravel().astype(np.float32)
    # The coast's rule by s; in the channel the same rule by height, blended to the bed's 0.9 by the
    # fan's bed weight (no step at the crease).
    coast = np.where(s < _beach.WET_TOP_S, 0.7, np.where(s < 0.0, 1.0, 0.0))
    bank = np.where(h >= _beach.WET_TOP_Y, 0.7, np.where(h >= WATER, 1.0, 0.0))
    w = bed_weight(hand, xs, ys)[0]
    wet = np.where(ch < co - 1e-4, bank + (0.9 - bank) * w, coast).ravel().astype(np.float32)
    return bake.make_masks(size, snow, wet, 0.0, None, str(out / f"{piece}.masks.png"))


def add_color0(path, hand):
    """Appends COLOR_0 (FLOAT VEC4, `fan_color`) to the river_bed, beach_sand and beach_wet primitives
    of every LOD in the exported GLB (runtime (x, y, z) is Blender (x, -z, y)). Border and joint-row
    vertices must keep the look of their neighbour pieces: rgb 1 and a 1 on plaster, the river's own
    tint and a 1 on gravel. Returns {slot: vertex count}."""
    data = pathlib.Path(path).read_bytes()
    jlen = struct.unpack_from("<I", data, 12)[0]
    j = json.loads(data[20:20 + jlen])
    boff = 20 + jlen
    blen = struct.unpack_from("<I", data, boff)[0]
    blob = bytearray(data[boff + 8:boff + 8 + blen])
    for n in j["nodes"]:
        if "mesh" not in n and "children" not in n:
            continue  # sockets
        assert not any(k in n for k in ("matrix", "rotation", "scale")) and all(
            abs(v) < 1e-9 for v in n.get("translation", (0, 0, 0))), f"{path}: node {n.get('name')} is transformed"
    mats = [m["name"] for m in j["materials"]]
    acc, views = j["accessors"], j["bufferViews"]
    counts = {}
    for mesh in j["meshes"]:
        if "_lod" not in mesh.get("name", ""):
            continue
        for prim in mesh["primitives"]:
            slot = mats[prim["material"]]
            if slot not in (BED, SAND, WET):
                continue
            a = acc[prim["attributes"]["POSITION"]]
            bv = views[a["bufferView"]]
            start = bv.get("byteOffset", 0) + a.get("byteOffset", 0)
            assert bv.get("byteStride", 12) == 12 and a["componentType"] == 5126
            p = np.frombuffer(bytes(blob[start:start + 12 * a["count"]]), "<f4").reshape(-1, 3).astype(np.float64)
            X, Y, Z = p[:, 0], -p[:, 2], p[:, 1]
            col = fan_color(hand, slot, X, Y, Z)
            assert np.all(col >= -1e-9) and np.all(col <= 1.0 + 1e-9), (slot, col.min(), col.max())
            col = np.clip(col, 0.0, 1.0)
            fixed = np.abs(np.array([hexgeo.dist_to_border(x, y) for x, y in zip(X, Y)])) < 1e-4
            if slot == BED:
                lxy = np.array([hexgeo.rotate_xy((x, y), -lane_of(hand)) for x, y in zip(X, Y)])
                fixed |= (np.abs(lxy[:, 0] - _river.JOINT_X) < 1e-4) & (np.abs(lxy[:, 1]) <= _river.BANK_TOP + 1e-4)
            want = (BED_TINT / BED_SLOT_TINT) if slot == BED else np.ones(3)
            if fixed.any():
                err = np.abs(col[fixed, :3] - want).max()
                assert err < 2e-3 and np.all(col[fixed, 3] > 1.0 - 1e-6), f"{slot}: seam vertex colour off by {err}"
            while len(blob) % 4:
                blob.append(0)
            vals = np.round(col, 6).astype("<f4").tobytes()
            views.append({"buffer": 0, "byteOffset": len(blob), "byteLength": len(vals), "target": 34962})
            blob += vals
            acc.append({"bufferView": len(views) - 1, "componentType": 5126, "count": a["count"], "type": "VEC4"})
            prim["attributes"]["COLOR_0"] = len(acc) - 1
            counts[slot] = counts.get(slot, 0) + int(a["count"])
    while len(blob) % 4:
        blob.append(0)
    j["buffers"] = [{"byteLength": len(blob)}]
    js = json.dumps(j, separators=(",", ":")).encode()
    js += b" " * (-len(js) % 4)
    total = 12 + 8 + len(js) + 8 + len(blob)
    pathlib.Path(path).write_bytes(struct.pack("<III", 0x46546C67, 2, total) + struct.pack("<II", len(js), 0x4E4F534A)
                                   + js + struct.pack("<II", len(blob), 0x004E4942) + bytes(blob))
    return counts


def check_np(hand):
    rng = np.random.default_rng(7)
    X = rng.uniform(-16, 16, 4000)
    Y = rng.uniform(-16, 16, 4000)
    h = np_height(hand, X, Y)[0]
    err = max(abs(h[i] - MP.height(hand, X[i], Y[i])) for i in range(len(X)))
    assert err < 1e-9, f"np_height differs from _mouth_profile by {err}"


# LOD inputs: (row max segment, lattice spacing, crease/kink s step, polyline tol, bisection depth,
# along-channel s step).
PARAMS = ((0.7, 0.85, 0.75, 0.01, 6, 0.75), (1.8, 2.6, 1.25, 0.02, 5, 1.6), (40.0, None, 2.0, 0.03, 4, 2.0))
SKIRT_ROWS = (4, 2, 1)


def build_base(piece, hand, generator, params=PARAMS, skirt_rows=SKIRT_ROWS):
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    MP.check_spots()
    check_np(hand)
    scene.reset_scene(piece)
    root = scene.empty(piece)
    mats = _beach.make_materials()
    mats[BED] = materials.pbr_material(BED, materials.srgb(_river.BED_COLOR), 0.85)
    lods, rings, errs = [], [], []
    for i in range(3):
        c, loop, err = cap(f"{piece}_lod{i}_cap", hand, params[i], mats)
        check_beach_edges(loop)
        uv.planar_cap_uv(c)
        rings.append(loop_key(c, loop))
        sk = skirt(f"{piece}_lod{i}_skirt", hand, loop, skirt_rows[i], mats)
        body = geom.join([c, sk], f"{piece}_lod{i}")
        body.parent = root
        lods.append(body)
        errs.append(err)
    assert all(r == rings[0] for r in rings), f"{piece}: border ring differs across LODs"
    tris = [geom.tri_count(o) for o in lods]
    jd = joint_delta(hand)
    print(f"{piece}: ring {len(rings[0])} vertices, joint delta {jd:.4f} m, tris per LOD {tris}, errors {errs}")
    assert tris[0] <= LOD0_MAX and tris[1] <= LOD1_MAX and tris[1] <= 0.5 * tris[0], tris
    assert tris[2] <= LOD2_RATIO * tris[0], tris
    if tris[2] > LOD2_MAX:
        print(f"{piece}: LOD2 {tris[2]} tris is over the {LOD2_MAX} target (the 20% lane-cut ceiling holds)")
    for o in lods:
        names = {o.data.materials[p.material_index].name for p in o.data.polygons}
        assert names == set(SLOTS), (o.name, names)
    col = collider.collider_from(lods[2], "hull", f"{piece}_collider")
    col.parent = root
    # The hull mixes cap and skirt vertices, so a face made only of skirt vertices (constant UV) gets a
    # zero tangent, which gltf-validator rejects. The collider is never drawn: give it an oblique
    # planar UV, non-degenerate on every hull face. Near-collinear ring points on the walls also give
    # sliver hull faces: dissolve them first.
    bm = bmesh.new()
    bm.from_mesh(col.data)
    bmesh.ops.dissolve_degenerate(bm, dist=0.002, edges=bm.edges[:])
    bmesh.ops.triangulate(bm, faces=bm.faces[:])
    bm.to_mesh(col.data)
    bm.free()
    col.data.update()
    for poly in col.data.polygons:
        poly.use_smooth = False
    for poly in col.data.polygons:
        for li in poly.loop_indices:
            co = col.data.vertices[col.data.loops[li].vertex_index].co
            col.data.uv_layers[0].data[li].uv = ((co.x + 16.0 + 0.31 * co.z) / 32.0, (co.y + 16.0 + 0.53 * co.z) / 32.0)
    _river.lane_sockets(root, (lane_of(hand),), cut=_river.BANK_TOP)
    texel = uv.measure_texel_density(lods[0], MASK_SIZE)
    if not skip_bake:
        masks(hand, out, piece)
    export.export_glb(root, str(out / f"{piece}.glb"))
    tinted = add_color0(out / f"{piece}.glb", hand)
    print(f"{piece}: COLOR_0 on {tinted}")
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                           device="none", samples=0, marginPx=0,
                           channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           reason="analytic masks (snow above water; wetness by s on the coast, by height in the "
                                  "channel, blended to the bed's 0.9 by the fan's bed weight; AO 1): no bake",
                           texelDensityPxPerM=texel, profileErrorM=errs, hand=hand, jointDeltaM=round(jd, 4),
                           rowsS=_beach.ROWS_S, extraRingS=list(_beach.EXTRA_S), lodTris=tris,
                           fanColour={"bedWeightRampM": WB_G1, "seam": WB_SEAM, "color0Vertices": tinted},
                           lod2MaxRatio=LOD2_RATIO, lodParams=[list(p) for p in params], skipped=skip_bake)
    return root, lods
