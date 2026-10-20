"""rail_road_crossing_center (P3, refs `assets-source/refs/rail` ref-3, owner A17): the single straight
track of `rail_straight_center` (edges 0 and 3) crossed at 60 degrees by a two-lane road (edges 1 and
4), on a level crossing with timber deck panels and four white field gates swung open.

Edge contracts. The rail is `_rail.track` along `_rail.straight_frames`, so its border section at
edges 0 and 3 is `_rail.section()` in every LOD (`_rail.border_section_errors`). The road meets a
`road_straight_edge` stub at r = 6 on edges 1 and 4: its arms start with exactly `_road.PROFILE` and
the stub's centre-line dash corners there (`_road_bend.joint_section_errors`), like every road centre.

Frame. The road runs along d = (cos -60, sin -60) toward edge 1 with lateral e = (cos 30, sin 30), so a
road-local (s, t) is the Blender point s d + t e (= `hexgeo.rotate_xy((s, t), 1)`), and a point's
across-track offset is its Blender y = -cos30 s + t / 2. Arm 4 is arm 1 turned by three edge steps.

Deck. Timber panels at the rail-head height RAIL_TOP (flush with the running tops) cover the
parallelogram |y| <= DECK_Y, |t| <= KERB_T, with a flangeway of FLANGE on each side of each rail head
over a floor FLOOR_DROP below the deck, and side faces down into the ballast. Sleepers wholly under
the deck are left out; the rest stay, under or beside it.

Arms. Each arm sweeps PROFILE from the joint (u = 0, s = 6) to the deck edge (u = 1, y = -DECK_Y), each
lateral point t on its own ramp s(t, u) = 6 - u (6 - s_deck(t)). A smoothstep w(u) lifts the asphalt
to the deck (crown flattened), the kerb tops to a LIP above it, and the verge to the ballast top, so at
the deck edge the verge meets the ballast shoulder flush; the sink keeps the profile's 31-degree slope
down to BED_SINK, moving outward as the verge rises. A small cap closes each kerb lip at the deck.

Gates (ref-3: four white field gates, two each side of the track, swung open parallel to it). Each
stands on the track-parallel line |y| = GATE_Y at a road edge (|t| = GATE_T), hinged on a post at the
road side, its leaf open along the track away from the road, so the road stays clear for cars. LOD1
keeps posts, stiles, top and bottom rails and the brace; LOD2 has no gates.

Masks are analytic, as `_rail`: the rail's crib and sleeper field, then the road arms, the deck
(plank seams along the track and panel joints darkened in A, flangeways darker) and the gate
footprints written over it.
"""
import json
import math
import pathlib
import sys

import bmesh
import bpy

import numpy as np

from lib import bake, budgets, collider, export, geom, hexgeo, materials, scene, uv

import _rail
import _road
import _road_bend

C30 = math.cos(math.radians(30.0))
D_DIR = (0.5, -C30)
E_DIR = (C30, 0.5)
JOINT = _road.CENTER_HALF
DECK_Z = _rail.RAIL_TOP
DECK_Y = 1.35
KERB_T = _road.ROAD_WIDTH / 2 + _road.KERB_W
FLANGE = 0.06
FLOOR_Z = DECK_Z - 0.06
DECK_BOTTOM = 0.10
LIP = 0.02
VERGE_END = _rail.BALLAST_TOP + 0.03  # the verge ends a turf edge above the ballast, never coplanar
EDGE_FOOT = _rail.BALLAST_TOP - 0.02
LOD2_LIFT = 0.002
Y_IN = _rail.RAIL_T - _rail.HEAD_W / 2 - FLANGE
Y_OUT = _rail.RAIL_T + _rail.HEAD_W / 2 + FLANGE
DECK_T = (-KERB_T, -3.0, -1.5, 0.0, 1.5, 3.0, KERB_T)
SINK_SLOPE = (_road.VERGE_TOP - _road.BED_SINK) / (_road.SINK_T - (_road.ROAD_WIDTH / 2 + _road.VERGE))
CROWN_Z = _road.BED_TOP + _road.CROWN
DASH_END = JOINT - _road_bend.JOINT_DASH  # the stub's joint dash runs on into the arm to here

GATE_Y, GATE_T = 3.075, 3.55
POST_HALF, KEEP_HALF, POST_FOOT = 0.09, 0.07, -0.35
POST_TOP, KEEP_TOP = 1.48, 1.0
LEAF_L, LEAF_GAP, STILE_W, LEAF_D = 3.0, 0.03, 0.10, 0.07
BAR_D = 0.05
BARS = ((0.30, 0.08), (0.52, 0.07), (0.74, 0.07), (0.98, 0.07), (1.27, 0.11))  # (centre z, height)
STILE_Z = (0.24, 1.34)

DECK, GATE = "rail_crossing_deck", "rail_crossing_gate"
SLOTS = _rail.SLOTS + _road.SLOTS + (DECK, GATE)
SLOT_INDEX = {s: i for i, s in enumerate(SLOTS)}
DECK_COLOR, DECK_ROUGH = (0.13, 0.085, 0.052), 0.8
GATE_COLOR, GATE_ROUGH = (0.72, 0.72, 0.68), 0.55
MASK_SIZE = _rail.MASK_SIZE
PLANKS_IN, SEAM_HALF, SEAM_AO, FLANGE_AO = 5, 0.035, 0.6, 0.4


def B(s, t, z):
    """Road-local (s, t, z) to Blender (arm 1 frame)."""
    return (s * D_DIR[0] + t * E_DIR[0], s * D_DIR[1] + t * E_DIR[1], z)


def x_at(t, y):
    """Blender x of the point with road-lateral t and across-track y."""
    return (t - 0.5 * y) / C30


def s_deck(t):
    return (DECK_Y + 0.5 * t) / C30


def smooth(u):
    return u * u * (3.0 - 2.0 * u)


class Mesh:
    """Vertex and face lists; each face is wound to face its `want` direction (as `_rail.Mesh`)."""

    def __init__(self):
        self.verts, self.faces = [], []

    def add(self, p):
        self.verts.append((float(p[0]), float(p[1]), float(p[2])))
        return len(self.verts) - 1

    def face(self, idx, slot, want):
        p = [self.verts[i] for i in idx]
        n = [0.0, 0.0, 0.0]
        for a, b in zip(p, p[1:] + p[:1]):
            n[0] += (a[1] - b[1]) * (a[2] + b[2])
            n[1] += (a[2] - b[2]) * (a[0] + b[0])
            n[2] += (a[0] - b[0]) * (a[1] + b[1])
        d = sum(n[i] * want[i] for i in range(3))
        assert abs(d) > 1e-12, ("face edge-on to its want direction", p, want)
        self.faces.append((tuple(idx) if d > 0 else tuple(reversed(idx)), SLOT_INDEX[slot]))

    def quad(self, pts, slot, want):
        self.face([self.add(p) for p in pts], slot, want)

    def extend(self, other, k=0, slot_map=None):
        """Append `other` rotated by k edge steps; slot_map maps its slot indices to ours."""
        base = len(self.verts)
        for v in other.verts:
            x, y = hexgeo.rotate_xy((v[0], v[1]), k)
            self.verts.append((x, y, v[2]))
        for f, m in other.faces:
            self.faces.append((tuple(base + i for i in f), slot_map[m] if slot_map else m))

    def box(self, lo, hi, slot, skip=()):
        """Axis-aligned box; `skip` names faces to leave out ('-x', '+x', '-y', '+y', '-z', '+z')."""
        c = {(i, j, k): self.add((hi[0] if i else lo[0], hi[1] if j else lo[1], hi[2] if k else lo[2]))
             for i in (0, 1) for j in (0, 1) for k in (0, 1)}
        sides = {"-x": (0, 0, (-1, 0, 0)), "+x": (0, 1, (1, 0, 0)), "-y": (1, 0, (0, -1, 0)),
                 "+y": (1, 1, (0, 1, 0)), "-z": (2, 0, (0, 0, -1)), "+z": (2, 1, (0, 0, 1))}
        for name, (axis, val, want) in sides.items():
            if name in skip:
                continue
            ring = [(0, 0), (1, 0), (1, 1), (0, 1)]
            idx = []
            for a, b in ring:
                key = [a, b]
                key.insert(axis, val)
                idx.append(c[tuple(key)])
            self.face(idx, slot, want)

    def obj(self, name, mats):
        bm = bmesh.new()
        vs = [bm.verts.new(v) for v in self.verts]
        for f, m in self.faces:
            bm.faces.new([vs[i] for i in f]).material_index = m
        bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
        bmesh.ops.triangulate(bm, faces=bm.faces)
        mesh = bpy.data.meshes.new(name)
        for slot in SLOTS:
            mesh.materials.append(mats[slot])
        bm.to_mesh(mesh)
        bm.free()
        return scene.link(bpy.data.objects.new(name, mesh))


# --- road arms ----------------------------------------------------------------------------------

HALF = [p for p in _road.PROFILE if p[0] >= 0.0]  # crown (0) out to the sink, 8 points


def arm_profile(u):
    """The arm's (t, z) section at ramp position u (module doc); PROFILE exactly at u = 0."""
    w = smooth(u)
    half = []
    for i, (t, z) in enumerate(HALF):
        if i <= 2:  # asphalt: crown flattens onto the deck
            half.append((t, z + w * (DECK_Z - z)))
        elif i <= 4:  # kerb tops
            half.append((t, _road.KERB_TOP + w * (DECK_Z + LIP - _road.KERB_TOP)))
        elif i <= 6:  # kerb outer foot and verge top
            half.append((t, _road.VERGE_TOP + w * (VERGE_END - _road.VERGE_TOP)))
        else:  # sink: same slope, down to BED_SINK
            vz = half[-1][1]
            half.append((HALF[6][0] + (vz - _road.BED_SINK) / SINK_SLOPE, _road.BED_SINK))
    return [(-t, z) for (t, z) in reversed(half[1:])] + half


def arm_s(t, u):
    return JOINT - u * (JOINT - s_deck(t))


def arm_rows(n):
    """Ramp positions: n even steps plus the row where the joint dash ends on the centre line."""
    ud = (JOINT - DASH_END) / (JOINT - s_deck(0.0))
    us = sorted({j / n for j in range(n + 1)} | {ud})
    return us, ud


def arm(n):
    """Arm 1 (edge 1) from the joint to the deck edge: profile sweep, kerb lip caps, paint dash."""
    m = Mesh()
    us, ud = arm_rows(n)
    rows = []
    for u in us:
        prof = arm_profile(u)
        rows.append([(m.add(B(arm_s(t, u), t, z)), t, z) for t, z in prof])
    for a, b in zip(rows, rows[1:]):
        for i in range(len(a) - 1):
            dt, dz = a[i + 1][1] - a[i][1], a[i + 1][2] - a[i][2]
            want = (-dz * E_DIR[0], -dz * E_DIR[1], dt)
            m.face((a[i][0], b[i][0], b[i + 1][0], a[i + 1][0]), _road.span_slot(a[i][1], a[i + 1][1]), want)
    # Kerb lip caps at the deck edge (u = 1), and the verge's turf edge down into the ballast, both
    # facing the track (+y).
    tc = HALF[6][0] + (VERGE_END - EDGE_FOOT) / SINK_SLOPE
    for sg in (-1.0, 1.0):
        t0, t1 = sg * (_road.ROAD_WIDTH / 2), sg * KERB_T
        m.quad([B(s_deck(t0), t0, DECK_Z), B(s_deck(t0), t0, DECK_Z + LIP),
                B(s_deck(t1), t1, DECK_Z + LIP), B(s_deck(t1), t1, DECK_Z)], _road.KERB, (0, 1, 0))
        tv = sg * HALF[6][0]
        m.face([m.add(B(s_deck(t1), t1, VERGE_END)), m.add(B(s_deck(tv), tv, VERGE_END)),
                m.add(B(s_deck(sg * tc), sg * tc, EDGE_FOOT)), m.add(B(s_deck(t1), t1, EDGE_FOOT))],
               _road.VERGE_SLOT, (0, 1, 0))
    # The stub's joint dash, carried up the ramp PAINT_Z - CROWN_Z above the crown.
    w = _road.PAINT_W / 2
    pr = [u for u in us if u <= ud + 1e-12]
    for ua, ub in zip(pr, pr[1:]):
        za = _road.PAINT_Z + smooth(ua) * (DECK_Z - CROWN_Z)
        zb = _road.PAINT_Z + smooth(ub) * (DECK_Z - CROWN_Z)
        m.quad([B(arm_s(-w, ua), -w, za), B(arm_s(w, ua), w, za),
                B(arm_s(w, ub), w, zb), B(arm_s(-w, ub), -w, zb)], _road.PAINT, (0, 0, 1))
    return m


# --- deck ---------------------------------------------------------------------------------------

def deck(lod):
    """Timber panels between and beside the rails, flangeway walls and floors, side faces. LOD2 (past
    400 m) is one slab LOD2_LIFT above the rail heads with its two end faces."""
    m = Mesh()
    if lod >= 2:
        z = DECK_Z + LOD2_LIFT
        for i in range(len(DECK_T) - 1):
            ta, tb = DECK_T[i], DECK_T[i + 1]
            m.quad([(x_at(ta, -DECK_Y), -DECK_Y, z), (x_at(tb, -DECK_Y), -DECK_Y, z),
                    (x_at(tb, DECK_Y), DECK_Y, z), (x_at(ta, DECK_Y), DECK_Y, z)], DECK, (0, 0, 1))
        for sg in (-1.0, 1.0):
            t = sg * KERB_T
            m.quad([(x_at(t, -DECK_Y), -DECK_Y, z), (x_at(t, DECK_Y), DECK_Y, z),
                    (x_at(t, DECK_Y), DECK_Y, DECK_BOTTOM), (x_at(t, -DECK_Y), -DECK_Y, DECK_BOTTOM)], DECK,
                   (sg * E_DIR[0], sg * E_DIR[1], 0))
        return m
    bands = [(-DECK_Y, -Y_OUT), (-Y_IN, Y_IN), (Y_OUT, DECK_Y)]
    for y0, y1 in bands:
        for i in range(len(DECK_T) - 1):
            ta, tb = DECK_T[i], DECK_T[i + 1]
            m.quad([(x_at(ta, y0), y0, DECK_Z), (x_at(tb, y0), y0, DECK_Z),
                    (x_at(tb, y1), y1, DECK_Z), (x_at(ta, y1), y1, DECK_Z)], DECK, (0, 0, 1))
        for y, sg in ((y0, -1.0), (y1, 1.0)):
            if abs(abs(y) - DECK_Y) < 1e-9:
                continue  # the outer edge meets the arm
            for i in range(len(DECK_T) - 1):
                ta, tb = DECK_T[i], DECK_T[i + 1]
                m.quad([(x_at(ta, y), y, DECK_Z), (x_at(tb, y), y, DECK_Z),
                        (x_at(tb, y), y, FLOOR_Z), (x_at(ta, y), y, FLOOR_Z)], DECK, (0, sg, 0))
        for sg in (-1.0, 1.0):
            t = sg * KERB_T
            m.quad([(x_at(t, y0), y0, DECK_Z), (x_at(t, y1), y1, DECK_Z),
                    (x_at(t, y1), y1, DECK_BOTTOM), (x_at(t, y0), y0, DECK_BOTTOM)], DECK,
                   (sg * E_DIR[0], sg * E_DIR[1], 0))
    for side in (-1.0, 1.0):
        lo, hi = sorted((side * Y_IN, side * Y_OUT))
        web = (side * (_rail.RAIL_T - _rail.WEB_HALF), side * (_rail.RAIL_T + _rail.WEB_HALF))
        for ya, yb in ((lo, min(web)), (max(web), hi)):
            m.quad([(x_at(-KERB_T, ya), ya, FLOOR_Z), (x_at(KERB_T, ya), ya, FLOOR_Z),
                    (x_at(KERB_T, yb), yb, FLOOR_Z), (x_at(-KERB_T, yb), yb, FLOOR_Z)], DECK, (0, 0, 1))
        for sg in (-1.0, 1.0):
            t = sg * KERB_T
            m.quad([(x_at(t, lo), lo, FLOOR_Z), (x_at(t, hi), hi, FLOOR_Z),
                    (x_at(t, hi), hi, DECK_BOTTOM), (x_at(t, lo), lo, DECK_BOTTOM)], DECK,
                   (sg * E_DIR[0], sg * E_DIR[1], 0))
    return m


# --- gates --------------------------------------------------------------------------------------

def gates_arm1():
    """(hinge x, leaf direction along x) of the two gates on arm 1's side of the track (y = -GATE_Y)."""
    return [(x_at(GATE_T, -GATE_Y), 1.0), (x_at(-GATE_T, -GATE_Y), -1.0)]


def gate(m, xh, dr, y, lod):
    """Hinge post, open leaf along +-x from it (five bars, two stiles, a brace) and a keep post."""
    def xs(a, b):
        return (min(xh + dr * a, xh + dr * b), max(xh + dr * a, xh + dr * b))

    m.box((xh - POST_HALF, y - POST_HALF, POST_FOOT), (xh + POST_HALF, y + POST_HALF, POST_TOP), GATE, skip=("-z",))
    a = POST_HALF + LEAF_GAP
    far = a + LEAF_L
    hy = LEAF_D / 2
    for x0, x1 in (xs(a, a + STILE_W), xs(far - STILE_W, far)):
        m.box((x0, y - hy, STILE_Z[0]), (x1, y + hy, STILE_Z[1]), GATE)
    bars = BARS if lod == 0 else (BARS[0], BARS[-1])
    bx0, bx1 = xs(a + STILE_W, far - STILE_W)
    for zc, h in bars:
        m.box((bx0, y - BAR_D / 2, zc - h / 2), (bx1, y + BAR_D / 2, zc + h / 2), GATE, skip=("-x", "+x"))
    # Brace from the foot of the hanging stile up to the top rail two thirds along.
    p0 = (xh + dr * (a + STILE_W), BARS[0][0])
    p1 = (xh + dr * (a + STILE_W + 0.62 * (LEAF_L - 2 * STILE_W)), BARS[-1][0])
    L = math.hypot(p1[0] - p0[0], p1[1] - p0[1])
    nx, nz = -(p1[1] - p0[1]) / L * 0.035, (p1[0] - p0[0]) / L * 0.035
    hb = BAR_D / 2 * 0.8
    corners = ((-1, -1), (1, -1), (1, 1), (-1, 1))
    c = [[m.add((p[0] + sx * nx, y + sy * hb, p[1] + sx * nz)) for sx, sy in corners] for p in (p0, p1)]
    for i in range(4):
        j = (i + 1) % 4
        sx, sy = (corners[i][0] + corners[j][0]) / 2, (corners[i][1] + corners[j][1]) / 2
        m.face((c[0][i], c[0][j], c[1][j], c[1][i]), GATE, (sx * nx, sy, sx * nz))
    if lod == 0:
        kx = xh + dr * (far + LEAF_GAP + KEEP_HALF)
        m.box((kx - KEEP_HALF, y - KEEP_HALF, POST_FOOT), (kx + KEEP_HALF, y + KEEP_HALF, KEEP_TOP), GATE, skip=("-z",))


def gates(lod):
    m = Mesh()
    if lod >= 2:
        return m
    for xh, dr in gates_arm1():
        gate(m, xh, dr, -GATE_Y, lod)
    return m


# --- rail ---------------------------------------------------------------------------------------

def sleeper_under_deck(x):
    """True when the whole sleeper footprint lies inside |t| <= KERB_T (it is hidden under the deck)."""
    reach = C30 * _rail.SLEEPER_HALF_W + 0.5 * _rail.SLEEPER_HALF_L
    return abs(C30 * x) + reach <= KERB_T


def sleepers():
    return [sl for sl in _rail.straight_sleepers() if not sleeper_under_deck(sl[0][0])]


def lod_mesh(lod, rail_segments, arm_rows_n):
    m = Mesh()
    rail = _rail.track(_rail.straight_frames(rail_segments), sleepers() if lod < 2 else None, top_only=lod == 1)
    m.extend(rail, 0, slot_map={_rail.SLOT_INDEX[s]: SLOT_INDEX[s] for s in _rail.SLOTS})
    half = Mesh()
    half.extend(arm(arm_rows_n))
    half.extend(gates(lod))
    m.extend(half, 0)
    m.extend(half, 3)
    m.extend(deck(lod))
    return m


# --- checks -------------------------------------------------------------------------------------

def checks(obj):
    errs = _rail.border_section_errors(obj, (0, 3)) + _rail.interior_errors(obj, (0, 3))
    return errs + _road_bend.joint_section_errors(obj, (1, 4))


def ramp_grades():
    """Steepest asphalt grade along each lateral line of the ramp (smoothstep peak 1.5 x mean)."""
    out = {}
    for t, z0 in ((-_road.ROAD_WIDTH / 2, _road.BED_TOP), (0.0, CROWN_Z), (_road.ROAD_WIDTH / 2, _road.BED_TOP)):
        out[f"{t:+.1f}"] = round(1.5 * (DECK_Z - z0) / (JOINT - s_deck(t)), 4)
    return out


# --- masks --------------------------------------------------------------------------------------

def masks_field():
    """(wet, ao) per texel in Blender pixel order: the rail field, then the arms, deck and gates."""
    wet, ao = _rail.masks_field("straight")
    n = MASK_SIZE
    wet, ao = wet.reshape(n, n).copy(), ao.reshape(n, n).copy()
    c = (np.arange(n, dtype=np.float64) + 0.5) / n * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    t = C30 * xs + 0.5 * ys
    s = 0.5 * xs - C30 * ys
    band = np.abs(ys) <= DECK_Y
    sink_max = arm_profile(1.0)[-1][0]
    road = (~band) & (np.abs(t) <= sink_max) & (np.abs(s) <= JOINT)
    wet = np.where(road, np.where(np.abs(t) <= KERB_T, 0.3, 0.7), wet)
    ao = np.where(road, 1.0, ao)
    dk = band & (np.abs(t) <= KERB_T)
    ay = np.abs(ys)
    flange = dk & (((ay >= Y_IN) & (ay < _rail.RAIL_T - _rail.HEAD_W / 2)) |
                   ((ay > _rail.RAIL_T + _rail.HEAD_W / 2) & (ay <= Y_OUT)))
    head = dk & (np.abs(ay - _rail.RAIL_T) <= _rail.HEAD_W / 2)
    wet = np.where(dk, 0.5, wet)
    ao = np.where(dk, 1.0, ao)
    # Plank seams along the track (PLANKS_IN planks between the rails, two beside each) and panel
    # joints on the deck's lateral lines; SEAM_HALF reaches the nearest texel centre on every row.
    pitch = 2 * Y_IN / PLANKS_IN
    seams_y = [-Y_IN + k * pitch for k in range(1, PLANKS_IN)] + [s * (Y_OUT + DECK_Y) / 2 for s in (-1, 1)]
    seam = np.zeros_like(dk)
    for sy in seams_y:
        seam |= np.abs(ys - sy) < SEAM_HALF
    for jt in DECK_T[1:-1]:
        seam |= np.abs(t - jt) < SEAM_HALF
    ao = np.where(dk & seam, SEAM_AO, ao)
    ao = np.where(flange, FLANGE_AO, ao)
    wet = np.where(head, 0.15, wet)
    for xh, dr in gates_arm1():
        for sg in (1.0, -1.0):  # arm 1's gates and their 180-degree copies
            x0, x1 = sorted((xh - dr * POST_HALF, xh + dr * (POST_HALF + 2 * LEAF_GAP + LEAF_L + 2 * KEEP_HALF)))
            if sg < 0:
                x0, x1 = -x1, -x0
            g = (xs >= x0 - 0.05) & (xs <= x1 + 0.05) & (np.abs(ys + sg * GATE_Y) <= POST_HALF + 0.05)
            wet = np.where(g, 0.4, wet)
            ao = np.where(g, 1.0, ao)
    return wet.ravel().astype(np.float32), ao.ravel().astype(np.float32)


# --- the piece ----------------------------------------------------------------------------------

def build_piece(piece, rail_segments, arm_segments, generator):
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    out.mkdir(parents=True, exist_ok=True)
    scene.reset_scene(piece)
    root = scene.empty(piece)
    mats = {
        _rail.BALLAST: materials.pbr_material(_rail.BALLAST, (0.205, 0.2, 0.21, 1.0), 0.95),
        _rail.SLEEPER: materials.pbr_material(_rail.SLEEPER, (*_rail.SLEEPER_COLOR, 1.0), _rail.SLEEPER_ROUGH),
        _rail.STEEL: materials.pbr_material(_rail.STEEL, (*_rail.STEEL_COLOR, 1.0), _rail.STEEL_ROUGH),
        _rail.HEAD: materials.pbr_material(_rail.HEAD, (*_rail.HEAD_COLOR, 1.0), _rail.HEAD_ROUGH),
        _road.ASPHALT: materials.pbr_material(_road.ASPHALT, materials.srgb("#3a3a3c"), 0.75),
        _road.VERGE_SLOT: materials.pbr_material(_road.VERGE_SLOT, materials.srgb("#4f7d32"), 0.9),
        _road.KERB: materials.pbr_material(_road.KERB, (*_road.KERB_COLOR, 1.0), 0.85),
        _road.PAINT: materials.pbr_material(_road.PAINT, materials.srgb("#e6e2d6"), 0.6),
        DECK: materials.pbr_material(DECK, (*DECK_COLOR, 1.0), DECK_ROUGH),
        GATE: materials.pbr_material(GATE, (*GATE_COLOR, 1.0), GATE_ROUGH),
    }
    lods = []
    for i, (nr, na) in enumerate(zip(rail_segments, arm_segments)):
        o = lod_mesh(i, nr, na).obj(f"{piece}_lod{i}", mats)
        _rail.lifted_uv(o)
        o.data.shade_smooth()
        o.data.set_sharp_from_angle(angle=math.radians(30.0))
        _road.verge_normals_up(o)
        o.parent = root
        lods.append(o)
        assert [mt.name for mt in o.data.materials] == list(SLOTS), [mt.name for mt in o.data.materials]
    tris = [geom.tri_count(o) for o in lods]
    print(f"{piece}: tris per LOD {tris}; ramp grades {ramp_grades()}")
    errs = [e for o in lods for e in checks(o)]
    for e in errs[:12]:
        print("CHECK:", e)
    assert not errs, f"{len(errs)} section errors"
    budgets.check(tris, "center", piece)

    signature = bake.receiver_signature([lods[0]])
    texel = uv.measure_texel_density(lods[0], MASK_SIZE)
    wet, ao = masks_field()
    bake.make_masks(MASK_SIZE, 1.0, wet, 0.0, None, str(out / f"{piece}.masks.png"))
    _, px = bake.load_png(str(out / f"{piece}.masks.png"))
    px[:, 3] = ao
    bake.write_png(MASK_SIZE, px, str(out / f"{piece}.masks.png"))

    col = collider.collider_from(lods[2], "box", f"{piece}_collider")
    col.parent = root
    for n, (k, kind, width) in enumerate(((0, "rail", _rail.LANE_WIDTH), (3, "rail", _rail.LANE_WIDTH),
                                          (1, "road", _road.ROAD_WIDTH), (4, "road", _road.ROAD_WIDTH))):
        c = hexgeo.lane_frame(k, 0.0, width)["center"]
        export.add_socket(f"socket-lane-{n}", (*c, 0.0), root, kind=kind, edge=k, offset=0.0, width=width)
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(
        str(out / "bake-report.json"), receiverSignature=signature, device="none", samples=0, marginPx=0,
        channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}}, texelDensityPxPerM=texel,
        reason="no Cycles bake: the masks are an analytic field (the rail's crib field, road arms, deck, gates)",
        masks={"snow": 1.0, "porosity": {"ballast": 0.9, "sleeper": 0.6, "rail": 0.15, "asphalt_kerb": 0.3,
                                         "verge": 0.7, "deck": 0.5, "gate": 0.4},
               "ao": {"crib": [0.78, 0.95], "flangeway": FLANGE_AO, "plankSeam": SEAM_AO, "other": 1.0}, "night": 0.0},
        crossing={"railLanes": [0, 3], "roadLanes": [1, 4], "angleDeg": 60.0, "deckTopM": DECK_Z,
                  "deckHalfAcrossTrackM": DECK_Y, "deckHalfAcrossRoadM": KERB_T, "flangewayM": FLANGE,
                  "floorM": FLOOR_Z, "kerbLipM": LIP, "rampGradeMax": ramp_grades(),
                  "sleepersKept": len(sleepers()), "sleepersUnderDeck": len(_rail.straight_sleepers()) - len(sleepers()),
                  "gates": {"count": 4, "state": "open, leaves along the track away from the road",
                            "lineAcrossTrackM": GATE_Y, "hingeRoadOffsetM": GATE_T, "leafM": LEAF_L}},
        lodTris=tris, skipped=False)
    print(json.dumps({"piece": piece, "tris": tris, "texel": texel}))
    return root, lods
