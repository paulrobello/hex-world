"""rail_junction_center (P3, refs `assets-source/refs/rail`, owner A17): a single-track right-hand
turnout. The straight runs edge 0 to edge 3 as `rail_straight_center`; the branch leaves the toe at
edge 0 on `rail_bend_60_center`'s 8 m arc about corner 0 and exits at edge 1. Lanes 0, 1 and 3.

Edge contract. At each lane edge the cross-section is `_rail.section()` in every LOD
(`_rail.border_section_errors`): edge 0 carries the straight ballast with the straight stock rail
(+y) and the curved stock rail (the branch's inner rail, which starts on y = -0.84 there), edge 3 the
straight ballast and both straight rails, edge 1 the branch ballast and both branch rails, all swept
exactly as `_rail.track` sweeps them. Every other part stops a few cm inside the border.

Rails (right-hand turnout, branch turning toward corner 0):
- stock rails: the straight +y rail (edge 0 to 3) and the curved branch inner rail (edge 0 to 1);
- switch blades: the straight route's -y rail and the branch's outer rail each start as a tapered
  blade on the gauge side of the opposite stock rail at BLADE_TIP, pushed off it until the route's
  own line clears it, top a few mm under the stock rail at the tip;
- frog: the straight -y rail and the branch outer rail cross at FROG (about 36 degrees, the 8 m
  radius); the rails beyond form the vee nose, the rails before end at the throat and run on as wing
  rails beside the nose legs, a flangeway apart;
- check rails a flangeway inside both stock rails opposite the frog;
- long timbers square to the main line under the switch and frog, then the straight's own sleepers
  and the bend's sleepers that clear them, shortened where a timber is in the way (`layout`; the
  timber count is the one that leaves the branch the smallest unsupported run, `best_timbers`);
- a ground frame (point lever) on the +y side by the toe, rodding to a stretcher bar between the
  blades. Every running surface is at or below `_rail.RAIL_TOP`.

Ballast. Two swept beds (the straight's and the branch's), each clipped per frame where the other
is higher (`clip_profile`), so the union reads as one bed with a crease and no coplanar overlap.

Masks are analytic on the planar UV0, as `_rail`: crib darkening between sleepers and timbers, A = 1
and lower porosity on sleepers, timbers, rails and the lever.
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

A = hexgeo.APOTHEM
R = _rail.BEND_R
CX, CY = _rail.BEND_C
RT = _rail.RAIL_T
TOP = _rail.RAIL_TOP
HW = _rail.HEAD_W
LANES = (0, 1, 3)
R_IN, R_OUT = R - RT, R + RT  # the branch's inner (curved stock) and outer rails

BLADE_TIP = 0.25  # s (m from edge 0 along the main line) of the blade tips
BLADE_LEN = 1.2  # taper length of a blade head, 2 cm to the full head width
BLADE_TIP_W, BLADE_DROP = 0.02, 0.008
BLADE_GAP = 0.004
FLANGE = 0.05  # clear gap between running edges at the frog and check rails
OFF = HW + FLANGE  # centre to centre of a wing or check rail and its running rail
NOSE = 0.14  # vee legs start this far past the crossing point, where their heads separate
THROAT = 0.21  # the closure rails end this far before it
WING_LEN, CHECK_HALF = 0.7, 0.75
SLEEPER_CLEAR = 0.06

LEVER = "rail_lever"
SLOTS = _rail.SLOTS + (LEVER,)
SLOT_INDEX = {s: i for i, s in enumerate(SLOTS)}
LEVER_COLOR, LEVER_ROUGH = (0.42, 0.045, 0.03), 0.55  # painted ground-frame red, linear
LEVER_S, LEVER_Y = 0.70, 2.6
MASK_SIZE = _rail.MASK_SIZE


def u(deg):
    return _rail.u(deg)


def main_xy(s, y=0.0):
    """The straight's point s m in from edge 0 at lateral y (Blender)."""
    return (A - s, y)


def arc_xy(r, th):
    d = u(th)
    return (CX + r * d[0], CY + r * d[1])


def arc_y_at_s(r, s):
    """y of the circle of radius r about corner 0 at x = A - s (upper branch)."""
    return CY + math.sqrt(r * r - s * s)


def frog():
    """The crossing of the straight's -y rail (y = -RT) and the branch outer rail: (s, theta)."""
    s = math.sqrt(R_OUT ** 2 - (CY - (-RT)) ** 2)
    th = math.degrees(math.atan2(-RT - CY, -s))
    return s, th


FROG_S, FROG_TH = frog()
FROG = main_xy(FROG_S, -RT)
FROG_ANGLE = FROG_TH - 90.0  # the branch outer rail's heading off the main line there


# --- mesh ---------------------------------------------------------------------------------------

class Mesh:
    """Vertex and face lists; each face is wound to face its `want` direction (as `_rail.Mesh`).
    Repeated vertex indices are dropped, so clipped sweeps can collapse rows onto one point."""

    def __init__(self):
        self.verts, self.faces = [], []

    def add(self, p):
        self.verts.append((float(p[0]), float(p[1]), float(p[2])))
        return len(self.verts) - 1

    def face(self, idx, slot, want):
        out = []
        for i in idx:
            if not out or out[-1] != i:
                out.append(i)
        while len(out) > 1 and out[0] == out[-1]:
            out.pop()
        if len(set(out)) < 3:
            return
        p = [self.verts[i] for i in out]
        n = [0.0, 0.0, 0.0]
        for a, b in zip(p, p[1:] + p[:1]):
            n[0] += (a[1] - b[1]) * (a[2] + b[2])
            n[1] += (a[2] - b[2]) * (a[0] + b[0])
            n[2] += (a[0] - b[0]) * (a[1] + b[1])
        if math.sqrt(sum(c * c for c in n)) < 1e-9:
            return
        d = sum(n[i] * want[i] for i in range(3))
        assert abs(d) > 1e-12, ("face edge-on to its want direction", p, want)
        self.faces.append((tuple(out) if d > 0 else tuple(reversed(out)), SLOT_INDEX[slot]))

    def sweep_rows(self, rows, frames, profiles, slot_of):
        """Quads between consecutive rows of vertex indices; profiles[s] are the (t, z) of row s."""
        for s in range(len(rows) - 1):
            lat = [(frames[s][1][j] + frames[s + 1][1][j]) / 2 for j in (0, 1)]
            prof = profiles[s]
            for i in range(len(prof) - 1):
                dt, dz = prof[i + 1][0] - prof[i][0], prof[i + 1][1] - prof[i][1]
                want = (-dz * lat[0], -dz * lat[1], dt)
                self.face((rows[s][i], rows[s + 1][i], rows[s + 1][i + 1], rows[s][i + 1]),
                          slot_of(prof[i], prof[i + 1]), want)

    def sweep(self, frames, profiles, slot_of, caps=False):
        """Frames (origin xy, lateral xy), one profile per frame (or one for all); profile point (t, z)
        sits at origin + t * lateral. `caps` closes both ends (rails that end inside the hex)."""
        if isinstance(profiles[0][0], (int, float)):
            profiles = [profiles] * len(frames)
        rows = [[self.add((o[0] + t * l[0], o[1] + t * l[1], z)) for t, z in p]
                for (o, l), p in zip(frames, profiles)]
        self.sweep_rows(rows, frames, profiles, slot_of)
        if caps:
            for end, sign in ((0, -1.0), (-1, 1.0)):
                a, b = frames[0][0], frames[1][0]
                if end == -1:
                    a, b = frames[-2][0], frames[-1][0]
                d = (b[0] - a[0], b[1] - a[1])
                ring = rows[end] + [rows[end][0]]
                self.face(ring[:-1], slot_of(profiles[end][1], profiles[end][2]), (sign * d[0], sign * d[1], 0))

    def obox(self, c, ax, half, slot, skip=()):
        """Oriented box: centre c (xyz), unit axes ax[0..2], half extents."""
        def at(i, j, k):
            return self.add(tuple(c[n] + i * half[0] * ax[0][n] + j * half[1] * ax[1][n] + k * half[2] * ax[2][n]
                                  for n in range(3)))
        v = {(i, j, k): at(i, j, k) for i in (-1, 1) for j in (-1, 1) for k in (-1, 1)}
        for axis in range(3):
            for sgn in (-1, 1):
                if (axis, sgn) in skip:
                    continue
                ring = [(-1, -1), (1, -1), (1, 1), (-1, 1)]
                idx = []
                for a, b in ring:
                    key = [a, b]
                    key.insert(axis, sgn)
                    idx.append(v[tuple(key)])
                self.face(idx, slot, tuple(sgn * ax[axis][n] for n in range(3)))

    def extend(self, other):
        base = len(self.verts)
        self.verts.extend(other.verts)
        for f, m in other.faces:
            self.faces.append((tuple(base + i for i in f), m))

    def obj(self, name, mats):
        bm = bmesh.new()
        vs = [bm.verts.new(v) for v in self.verts]
        for f, m in self.faces:
            bm.faces.new([vs[i] for i in f]).material_index = m
        bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
        bmesh.ops.dissolve_degenerate(bm, dist=1e-6, edges=bm.edges)
        bmesh.ops.triangulate(bm, faces=bm.faces)
        mesh = bpy.data.meshes.new(name)
        for slot in SLOTS:
            mesh.materials.append(mats[slot])
        bm.to_mesh(mesh)
        bm.free()
        return scene.link(bpy.data.objects.new(name, mesh))


# --- ballast: two beds, each clipped where the other is higher ----------------------------------

HALF = [(abs(t), z) for t, z in _rail.BALLAST_PROFILE if t >= 0.0]


def bed_z(t):
    t = abs(t)
    if t > _rail.SINK_T + 1e-9:
        return -math.inf
    if t <= _rail.SHOULDER_T:
        return _rail.BALLAST_TOP
    for (t0, z0), (t1, z1) in zip(HALF, HALF[1:]):
        if t <= t1 + 1e-12:
            return z0 + (z1 - z0) * (t - t0) / (t1 - t0) if t1 > t0 else z0
    return HALF[-1][1]


def z_main(x, y):
    return bed_z(y) if abs(x) <= A + 1e-6 else -math.inf


def z_branch(x, y):
    dx, dy = x - CX, y - CY
    th = math.degrees(math.atan2(dy, dx)) % 360.0
    if not (_rail.BEND_FROM - 1e-9 <= th <= _rail.BEND_FROM + _rail.BEND_TURN + 1e-9):
        return -math.inf
    return bed_z(math.hypot(dx, dy) - R)


def clip_profile(origin, lat, prof, own, other, order, wins_tie):
    """Walk `prof` (t, z) in index `order` from its far end; the first point under `other` (a tie
    counts as under unless `wins_tie`) and every later one collapse onto the crossing, found by
    bisection on that span. Returns (points or None when nothing shows, collapsed indices)."""
    def at(t):
        return (origin[0] + t * lat[0], origin[1] + t * lat[1])

    def hidden(t, z):
        zo = other(*at(t))
        return z < zo - 1e-6 if wins_tie else z <= zo + 1e-6

    pts = list(prof)
    t0, z0 = pts[order[0]]
    if z0 < other(*at(t0)) - 1e-6:  # strict here, so the toe frame (a tie all across) keeps a row
        return None, ()
    for a, b in zip(order, order[1:]):
        if hidden(*pts[b]):
            (ta, za), (tb, zb) = pts[a], pts[b]
            lo, hi = 0.0, 1.0
            for _ in range(48):
                mid = (lo + hi) / 2
                if hidden(ta + (tb - ta) * mid, za + (zb - za) * mid):
                    hi = mid
                else:
                    lo = mid
            cross = (ta + (tb - ta) * lo, za + (zb - za) * lo)
            rest = order[order.index(b):]
            for i in rest:
                pts[i] = cross
            return pts, tuple(rest)
    return pts, ()


def clipped_bed(m, frames, own, other, order, wins_tie):
    prof = _rail.BALLAST_PROFILE
    rows, keep, profs = [], [], []
    for o, l in frames:
        pts, collapsed = clip_profile(o, l, prof, own, other, order, wins_tie)
        if pts is None:
            rows.append(None)
            continue
        row, cross = [], None
        for i, (t, z) in enumerate(pts):
            if i in collapsed:
                if cross is None:
                    cross = m.add((o[0] + t * l[0], o[1] + t * l[1], z))
                row.append(cross)
            else:
                row.append(m.add((o[0] + t * l[0], o[1] + t * l[1], z)))
        rows.append(row)
    # Sweep runs of consecutive kept frames, with the unclipped profile's spans for the winding.
    run = []
    for i, row in enumerate(rows + [None]):
        if row is not None:
            run.append(i)
            continue
        if len(run) > 1:
            m.sweep_rows([rows[j] for j in run], [frames[j] for j in run], [prof] * len(run),
                         lambda a, b: _rail.BALLAST)
        run = []
    return sum(r is not None for r in rows)


# --- paths --------------------------------------------------------------------------------------

def main_frames(s_list):
    return [(main_xy(s), (0.0, 1.0)) for s in s_list]


def main_s(dense_step, far_segs, dense_to=10.0):
    n = max(1, round(dense_to / dense_step))
    s = [dense_to * i / n for i in range(n + 1)]
    rest = 2 * A - dense_to
    s += [dense_to + rest * i / far_segs for i in range(1, far_segs + 1)]
    return s


def arc_frames(th_list, r=R):
    return [(arc_xy(r, th), u(th)) for th in th_list]


def bend_th(n):
    return [_rail.BEND_FROM + _rail.BEND_TURN * i / n for i in range(n + 1)]


def polyline_frames(pts, ref=(0.0, 1.0)):
    """Mitred frames along a 2D polyline; laterals point to the `ref` side."""
    frames = []
    for i, p in enumerate(pts):
        ds = []
        for a, b in ((i - 1, i), (i, i + 1)):
            if 0 <= a and b < len(pts):
                d = (pts[b][0] - pts[a][0], pts[b][1] - pts[a][1])
                ln = math.hypot(*d)
                ds.append((d[0] / ln, d[1] / ln))
        d = (sum(v[0] for v in ds), sum(v[1] for v in ds))
        ln = math.hypot(*d)
        d = (d[0] / ln, d[1] / ln)
        lat = (-d[1], d[0])
        if lat[0] * ref[0] + lat[1] * ref[1] < 0:
            lat = (-lat[0], -lat[1])
        if len(ds) == 2:
            lat = tuple(c / max(0.5, ds[0][0] * d[0] + ds[0][1] * d[1]) for c in lat)
        frames.append((p, lat))
    return frames


def head_profile(w, top):
    """`_rail.RAIL_PROFILE` about t = 0 with head width w and head top `top`."""
    web = min(_rail.WEB_HALF, 0.3 * w)
    return [(-web, _rail.SLEEPER_TOP), (-web, top - 0.05), (-w / 2, top - 0.035), (-w / 2, top),
            (w / 2, top), (w / 2, top - 0.035), (web, top - 0.05), (web, _rail.SLEEPER_TOP)]


def blade_w(s):
    return BLADE_TIP_W + (HW - BLADE_TIP_W) * min(1.0, max(0.0, (s - BLADE_TIP) / BLADE_LEN))


def blade_top(s):
    return TOP - BLADE_DROP * (1.0 - min(1.0, max(0.0, (s - BLADE_TIP) / (0.5 * BLADE_LEN))))


def blade_need(s):
    return (HW + blade_w(s)) / 2 + BLADE_GAP


def straight_rail_y(s):
    """The straight route's -y rail: on y = -RT once clear of the curved stock rail, else pushed to
    its gauge side."""
    return max(-RT, arc_y_at_s(R_IN, s) + blade_need(s))


def branch_rail_y(s):
    """The branch outer rail: on r = R_OUT once clear of the straight +y stock rail, else pushed to
    its gauge side."""
    return min(arc_y_at_s(R_OUT, s), RT - blade_need(s))


def blade_s(step, until):
    n = max(1, round((until - BLADE_TIP) / step))
    return [BLADE_TIP + (until - BLADE_TIP) * i / n for i in range(n + 1)]


def a_before(lod):
    """Straight-route -y rail from its blade tip to the throat, then the wing rail on r = R_OUT - OFF
    beside the branch's vee leg, flared at its end."""
    step = (0.1, 0.3, 0.6)[lod]
    pts = [main_xy(s, straight_rail_y(s)) for s in blade_s(step, BLADE_TIP + BLADE_LEN)]
    pts.append(main_xy(FROG_S - THROAT, -RT))
    r_w = R_OUT - OFF
    th0 = math.degrees(math.atan2(-RT - CY, -(FROG_S - THROAT))) + 0.0
    th0 = max(th0, math.degrees(math.atan2(-RT - CY, -math.sqrt(max(r_w ** 2 - (CY + RT) ** 2, 0.0)))))
    th1 = FROG_TH + math.degrees(WING_LEN / r_w)
    for th in (th0 + (th1 - th0) * i / 3 for i in range(1, 4)):
        pts.append(arc_xy(r_w, th))
    pts.append(arc_xy(r_w - 0.05, th1 + math.degrees(0.25 / r_w)))
    return pts


def b_before(lod):
    """Branch outer rail from its blade tip to the throat, then the wing rail on y = -RT + OFF beside
    the straight's vee leg, flared at its end."""
    step = (0.1, 0.3, 0.6)[lod]
    ss = blade_s(step, BLADE_TIP + BLADE_LEN)
    s_throat = FROG_S - THROAT * math.cos(math.radians(FROG_ANGLE))
    n = (8, 3, 2)[lod]
    ss += [ss[-1] + (s_throat - ss[-1]) * i / n for i in range(1, n + 1)]
    pts = [main_xy(s, branch_rail_y(s)) for s in ss]
    y_w = -RT + OFF
    pts.append(main_xy(FROG_S + WING_LEN, y_w))
    pts.append(main_xy(FROG_S + WING_LEN + 0.25, y_w + 0.05))
    return pts


def a_after_frames(lod):
    s0 = FROG_S + NOSE
    n = (2, 1, 1)[lod]
    return main_frames([s0 + (2 * A - s0) * i / n for i in range(n + 1)])


def b_after_th(lod):
    th0 = FROG_TH + math.degrees(NOSE / R_OUT)
    full = bend_th((32, 12, 4)[lod])
    return [th0] + [th for th in full if th > th0 + 0.5]


def check_rails(lod):
    """(points, ref) of the check rail inside the straight +y stock rail and inside the curved stock
    rail, opposite the frog, with flared ends."""
    y = RT - OFF
    s0, s1 = FROG_S - CHECK_HALF, FROG_S + CHECK_HALF
    main = [main_xy(s0 - 0.25, y - 0.05), main_xy(s0, y), main_xy(s1, y), main_xy(s1 + 0.25, y - 0.05)]
    r = R_IN + OFF
    th_c = FROG_TH  # radially opposite the frog, where a branch wheelset spans it
    dth = math.degrees(CHECK_HALF / r)
    ths = [th_c - dth + 2 * dth * i / (2 if lod == 0 else 1) for i in range((2 if lod == 0 else 1) + 1)]
    flare = math.degrees(0.25 / r)
    arc = [arc_xy(r + 0.05, ths[0] - flare)] + [arc_xy(r, th) for th in ths] + [arc_xy(r + 0.05, ths[-1] + flare)]
    return [(main, (0.0, 1.0)), (arc, None)]


def rail_slot(a, b):
    return _rail.HEAD if abs(a[1] - b[1]) < 1e-9 and a[1] >= TOP - BLADE_DROP - 1e-9 else _rail.STEEL


def add_rail(m, pts, ref=(0.0, 1.0), top_fn=None, w_fn=None, s_of=None, caps=True):
    frames = polyline_frames(pts, ref)
    profs = []
    for i, (p, _) in enumerate(frames):
        s = A - p[0]
        w = w_fn(s) if w_fn and i < len(s_of or pts) and (s_of is None or s_of[i]) else HW
        top = top_fn(s) if top_fn and (s_of is None or s_of[i]) else TOP
        profs.append(head_profile(w, top))
    m.sweep(frames, profs, rail_slot, caps=caps)


def frog_nose(m):
    """A low wedge filling the vee between the two legs' starts."""
    pa = main_xy(FROG_S + NOSE, -RT)
    pb = arc_xy(R_OUT, FROG_TH + math.degrees(NOSE / R_OUT))
    tri = [FROG, pa, pb]
    z0, z1 = _rail.SLEEPER_TOP, TOP - 0.005
    top = [m.add((p[0], p[1], z1)) for p in tri]
    bot = [m.add((p[0], p[1], z0)) for p in tri]
    m.face(top, _rail.HEAD, (0, 0, 1))
    cx = sum(p[0] for p in tri) / 3
    cy = sum(p[1] for p in tri) / 3
    for i in range(3):
        j = (i + 1) % 3
        mx, my = (tri[i][0] + tri[j][0]) / 2 - cx, (tri[i][1] + tri[j][1]) / 2 - cy
        m.face((bot[i], bot[j], top[j], top[i]), _rail.STEEL, (mx, my, 0.0))


# --- sleepers and timbers -----------------------------------------------------------------------

def rect(c, along, half_along, half_across):
    """2D rectangle: centre, unit `along` axis (the sleeper's width direction), half extents."""
    return (c, along, half_along, half_across)


def rect_overlap(a, b, gap=0.0):
    def axes(r):
        d = r[1]
        return [d, (-d[1], d[0])]

    def proj(r, ax):
        c, d, ha, hc = r
        n = (-d[1], d[0])
        cc = c[0] * ax[0] + c[1] * ax[1]
        ext = ha * abs(d[0] * ax[0] + d[1] * ax[1]) + hc * abs(n[0] * ax[0] + n[1] * ax[1])
        return cc - ext, cc + ext

    for ax in axes(a) + axes(b):
        lo_a, hi_a = proj(a, ax)
        lo_b, hi_b = proj(b, ax)
        if hi_a + gap <= lo_b or hi_b + gap <= lo_a:
            return False
    return True


SHORT_HALF = RT + 0.21  # a branch sleeper shortened to just past both rails' feet


def main_sleepers():
    """`_rail.straight_sleepers()` ordered from edge 0 (largest x) inward."""
    return sorted(_rail.straight_sleepers(), key=lambda sl: -sl[0][0])


def timber_rects(n):
    """The long timbers: the first n straight sleeper positions, each from the straight sleeper's +y
    end to past the curved stock rail."""
    out = []
    for o, _, _ in main_sleepers()[:n]:
        s = A - o[0]
        y_hi = _rail.SLEEPER_HALF_L
        y_lo = arc_y_at_s(R_IN, s) - (_rail.SLEEPER_HALF_L - RT)
        out.append(rect((o[0], (y_hi + y_lo) / 2), (1.0, 0.0), _rail.SLEEPER_HALF_W, (y_hi - y_lo) / 2))
    return out


def rail_strips():
    """Rectangles a branch sleeper may not cross: the straight's two rails beyond the toe."""
    return [rect((0.0, RT), (0.0, 1.0), 0.12, A), rect((0.0, -RT), (0.0, 1.0), 0.12, A)]


def layout(n):
    """(timbers, straight sleepers, branch sleepers with their theta) for n long timbers. Each of the
    bend's own sleepers is kept at full length when it clears the rest, else shortened to SHORT_HALF
    about the branch centreline when that clears, else left out."""
    tim = timber_rects(n)
    straight = [rect(o, (1.0, 0.0), _rail.SLEEPER_HALF_W, _rail.SLEEPER_HALF_L) for o, _, _ in main_sleepers()[n:]]
    blocked = tim + straight + rail_strips()
    branch = []
    for j, (o, lat, tan) in enumerate(_rail.bend_sleepers()):
        th = _rail.sleeper_angles_or_x("bend")[j]
        for half in (_rail.SLEEPER_HALF_L, SHORT_HALF):
            r = rect(o, tan, _rail.SLEEPER_HALF_W, half)
            if not any(rect_overlap(r, b, SLEEPER_CLEAR) for b in blocked):
                branch.append((r, th))
                break
    return tim, straight, branch


def branch_gap_deg(n):
    """The widest run of the branch centreline with no timber or branch sleeper under it (degrees
    about corner 0), from the edge-0 border to the edge-1 border."""
    tim, _, branch = layout(n)
    th = [_rail.BEND_FROM, _rail.BEND_FROM + _rail.BEND_TURN] + [t for _, t in branch]
    for (c, _, _, _) in tim:
        cx = (c[0] - CX) / R
        if -1.0 <= cx <= 1.0:
            th.append(math.degrees(math.acos(cx)))
    th.sort()
    return max(b - a for a, b in zip(th, th[1:]))


def best_timbers():
    """The timber count (at least to the timber under the frog, at most 10: they grow to 7 m on the 8 m
    radius) that leaves the branch the smallest gap."""
    first = next(i for i, (o, _, _) in enumerate(main_sleepers()) if A - o[0] > FROG_S - _rail.SLEEPER_HALF_W) + 1
    return min(range(first, 11), key=lambda n: (round(branch_gap_deg(n), 3), n))


TIMBERS = best_timbers()
TIMBER_RECTS, STRAIGHT_RECTS, BRANCH = layout(TIMBERS)


def timbers():
    return TIMBER_RECTS


def straight_rects():
    return STRAIGHT_RECTS


def branch_rects():
    return [r for r, _ in BRANCH]


def add_sleeper(m, r, top_only):
    c, d, ha, hc = r
    ax = ((d[0], d[1], 0.0), (-d[1], d[0], 0.0), (0.0, 0.0, 1.0))
    zc = (_rail.SLEEPER_TOP + _rail.SLEEPER_BOTTOM) / 2
    hz = (_rail.SLEEPER_TOP - _rail.SLEEPER_BOTTOM) / 2
    if top_only:
        p = [(c[0] + i * ha * d[0] - j * hc * d[1], c[1] + i * ha * d[1] + j * hc * d[0])
             for i, j in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
        m.face([m.add((x, y, _rail.SLEEPER_TOP)) for x, y in p], _rail.SLEEPER, (0, 0, 1))
        return
    m.obox((c[0], c[1], zc), ax, (ha, hc, hz), _rail.SLEEPER, skip=((2, -1),))


# --- ground frame, rodding and stretcher bar ----------------------------------------------------

def lever_parts(m, lod):
    x0 = A - LEVER_S
    X, Y, Z = (1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0)
    m.obox((x0, LEVER_Y, -0.09), (X, Y, Z), (0.3, 0.25, 0.21), _rail.SLEEPER, skip=((2, -1),))  # base timber
    m.obox((x0, LEVER_Y, 0.285), (X, Y, Z), (0.08, 0.07, 0.165), _rail.STEEL, skip=((2, -1),))  # frame stand
    lean = math.radians(28.0)  # the lever leans inward (toward edge 3), pivoted at the stand top
    ax = ((math.cos(lean), 0.0, math.sin(lean)), Y, (-math.sin(lean), 0.0, math.cos(lean)))
    length = 0.95
    c = (x0 - math.sin(lean) * length / 2, LEVER_Y, 0.40 + math.cos(lean) * length / 2)
    m.obox(c, ax, (0.022, 0.022, length / 2), LEVER)
    m.obox((x0 + 0.13, LEVER_Y, 0.47), (X, Y, Z), (0.06, 0.05, 0.05), LEVER)  # counterweight
    if lod > 0:
        return
    xs = A - 0.693  # the stretcher bar and rodding sit between the first two timbers
    y_a, y_b = straight_rail_y(0.693), branch_rail_y(0.693)
    m.obox((xs, (y_a + y_b) / 2, 0.335), (X, Y, Z), (0.025, (y_b - y_a) / 2 - 0.012, 0.015), _rail.STEEL)
    m.obox((xs, (RT - 0.06 + LEVER_Y - 0.07) / 2, 0.255), (X, Y, Z), (0.015, (LEVER_Y - 0.07 - RT + 0.06) / 2, 0.015),
           _rail.STEEL)


# --- LODs ---------------------------------------------------------------------------------------

def lod_mesh(lod):
    m = Mesh()
    main_bed = main_frames(main_s((0.4, 1.0, 2.5)[lod], (5, 3, 1)[lod]))
    br_frames = _rail.bend_frames((32, 12, 4)[lod])
    m.sweep(main_bed, _rail.BALLAST_PROFILE, lambda a, b: _rail.BALLAST)
    order_br = list(range(len(_rail.BALLAST_PROFILE)))  # from the inner sink outward
    clipped_bed(m, br_frames, z_branch, z_main, order_br, wins_tie=False)

    # Stock rails, swept exactly as `_rail.track` sweeps them, so the border sections match.
    m.sweep(main_frames([2 * A * i / (4, 2, 1)[lod] for i in range((4, 2, 1)[lod] + 1)]), _rail.rail_profile(1),
            lambda a, b: _rail.rail_slot(a, b))
    m.sweep(_rail.bend_frames((32, 12, 4)[lod]), _rail.rail_profile(-1), lambda a, b: _rail.rail_slot(a, b))
    m.sweep(a_after_frames(lod), _rail.rail_profile(-1), lambda a, b: _rail.rail_slot(a, b))
    m.sweep(arc_frames(b_after_th(lod)), _rail.rail_profile(1), lambda a, b: _rail.rail_slot(a, b))
    if lod == 2:
        # Far LOD: the closure rails as plain rails from where they clear the stock rails.
        add_rail(m, [main_xy(BLADE_TIP + BLADE_LEN, -RT), main_xy(FROG_S - THROAT, -RT)], caps=False)
        ths = [math.degrees(math.atan2(arc_y_at_s(R_OUT, s) - CY, -s)) for s in (1.4, FROG_S - THROAT)]
        add_rail(m, [arc_xy(R_OUT, th) for th in ths], caps=False)
        return m
    pa = a_before(lod)
    nb = len(blade_s((0.1, 0.3, 0.6)[lod], BLADE_TIP + BLADE_LEN))
    add_rail(m, pa, top_fn=blade_top, w_fn=blade_w, s_of=[i < nb for i in range(len(pa))])
    pb = b_before(lod)
    add_rail(m, pb, top_fn=blade_top, w_fn=blade_w, s_of=[i < nb for i in range(len(pb))])
    frog_nose(m)
    for pts, ref in check_rails(lod):
        if ref is None:
            c = (CX, CY)
            mid = pts[len(pts) // 2]
            ref = (mid[0] - c[0], mid[1] - c[1])
        add_rail(m, pts, ref=ref)
    for r in timbers():
        add_sleeper(m, r, lod == 1)
    for r in straight_rects():
        add_sleeper(m, r, lod == 1)
    for r in branch_rects():
        add_sleeper(m, r, lod == 1)
    lever_parts(m, lod)
    return m


# --- checks -------------------------------------------------------------------------------------

def running_surface_errors(obj):
    """Nothing in the track's swept band rises above the rail top (the train rides at RAIL_TOP)."""
    errs = []
    for v in obj.data.vertices:
        if v.co.z > TOP + 1e-5:
            x, y = v.co.x, v.co.y
            near_main = abs(y) < 1.5
            near_branch = abs(math.hypot(x - CX, y - CY) - R) < 1.5
            if near_main or near_branch:
                errs.append(f"{obj.name}: vertex {tuple(v.co)} above the rail top in the swept band")
    return errs[:5]


def checks(obj):
    return (_rail.border_section_errors(obj, LANES) + _rail.interior_errors(obj, LANES)
            + running_surface_errors(obj))


# --- masks --------------------------------------------------------------------------------------

def masks_field():
    """(wet, ao) per texel in Blender pixel order (row 0 = V 0), as module doc."""
    c = (np.arange(MASK_SIZE, dtype=np.float64) + 0.5) / MASK_SIZE * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    dist_main = np.where(np.abs(xs) <= A, np.abs(ys), np.inf)
    rr = np.hypot(xs - CX, ys - CY)
    th = np.degrees(np.arctan2(ys - CY, xs - CX)) % 360.0
    on_br = (th >= _rail.BEND_FROM) & (th <= _rail.BEND_FROM + _rail.BEND_TURN)
    dist_br = np.where(on_br, np.abs(rr - R), np.inf)
    track = np.minimum(dist_main, dist_br)
    rects = timbers() + straight_rects() + branch_rects()
    gap = np.full_like(xs, np.inf)
    for (cx, cy), d, ha, hc in rects:
        px, py = xs - cx, ys - cy
        a = np.abs(px * d[0] + py * d[1]) - ha
        b = np.abs(-px * d[1] + py * d[0]) - hc
        out = np.hypot(np.clip(a, 0, None), np.clip(b, 0, None)) + np.minimum(np.maximum(a, b), 0.0)
        gap = np.minimum(gap, out)
    crib = 0.78 + 0.17 * np.clip(gap / 0.2, 0.0, 1.0)
    ao = np.ones_like(xs)
    ao = np.where(track <= _rail.SHOULDER_T, 0.95, ao)
    ao = np.where((track <= _rail.SLEEPER_HALF_L) | (gap <= 0.3), np.minimum(ao, crib), ao)
    sleeper = gap <= 0.0
    rail = (np.abs(dist_main - RT) <= 0.06) | (np.abs(dist_br - RT) <= 0.06)
    lever = (np.abs(xs - (A - LEVER_S)) <= 0.32) & (np.abs(ys - LEVER_Y) <= 0.27)
    ao = np.where(sleeper | rail | lever, 1.0, ao)
    wet = np.full_like(xs, 0.9)
    wet = np.where(sleeper | lever, 0.6, wet)
    wet = np.where(rail, 0.15, wet)
    return wet.ravel().astype(np.float32), ao.ravel().astype(np.float32)


# --- the piece ----------------------------------------------------------------------------------

def build_piece(piece, generator):
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    out.mkdir(parents=True, exist_ok=True)
    scene.reset_scene(piece)
    root = scene.empty(piece)
    mats = {
        _rail.BALLAST: materials.pbr_material(_rail.BALLAST, (0.205, 0.2, 0.21, 1.0), 0.95),
        _rail.SLEEPER: materials.pbr_material(_rail.SLEEPER, (*_rail.SLEEPER_COLOR, 1.0), _rail.SLEEPER_ROUGH),
        _rail.STEEL: materials.pbr_material(_rail.STEEL, (*_rail.STEEL_COLOR, 1.0), _rail.STEEL_ROUGH),
        _rail.HEAD: materials.pbr_material(_rail.HEAD, (*_rail.HEAD_COLOR, 1.0), _rail.HEAD_ROUGH),
        LEVER: materials.pbr_material(LEVER, (*LEVER_COLOR, 1.0), LEVER_ROUGH),
    }
    lods = []
    for i in range(3):
        o = lod_mesh(i).obj(f"{piece}_lod{i}", mats)
        _rail.lifted_uv(o)
        o.data.shade_smooth()
        o.data.set_sharp_from_angle(angle=math.radians(30.0))
        o.parent = root
        lods.append(o)
        assert [mt.name for mt in o.data.materials] == list(SLOTS), [mt.name for mt in o.data.materials]
    tris = [geom.tri_count(o) for o in lods]
    nb = len(BRANCH)
    n_short = sum(1 for r, _ in BRANCH if r[3] < _rail.SLEEPER_HALF_L)
    gap = branch_gap_deg(TIMBERS)
    print(f"{piece}: tris per LOD {tris}; frog s {FROG_S:.3f} m at {FROG_ANGLE:.2f} deg; "
          f"timbers {TIMBERS}, straight sleepers {len(straight_rects())}, branch sleepers {nb} "
          f"({n_short} short), widest branch gap {gap:.2f} deg ({math.radians(gap) * R:.2f} m)")
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
    for n, k in enumerate(LANES):
        c = hexgeo.lane_frame(k, 0.0, _rail.LANE_WIDTH)["center"]
        export.add_socket(f"socket-lane-{n}", (*c, 0.0), root, kind="rail", edge=k, offset=0.0,
                          width=_rail.LANE_WIDTH)
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    sep = lambda s: R - math.sqrt(R * R - s * s)  # noqa: E731  branch centreline offset from the main line
    foul = math.sqrt(R * R - (R - 2.5) ** 2)
    routes = [{"from": 0, "to": 3, "kind": "straight", "lengthM": 2 * A},
              {"from": 0, "to": 1, "kind": "bend", "centre": [CX, CY], "radiusM": R, "turnDeg": _rail.BEND_TURN,
               "arcLengthM": R * math.radians(_rail.BEND_TURN)}]
    bake.write_bake_report(
        str(out / "bake-report.json"), receiverSignature=signature, device="none", samples=0, marginPx=0,
        channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}}, texelDensityPxPerM=texel,
        reason="no Cycles bake: the masks are an analytic field of the sleepers, timbers, rails and lever",
        masks={"snow": 1.0, "porosity": {"ballast": 0.9, "sleeper_timber_lever": 0.6, "rail": 0.15},
               "ao": {"crib": [0.78, 0.95], "shoulder": 0.95, "sleeper_rail_lever": 1.0}, "night": 0.0},
        turnout={"lanes": list(LANES), "toeEdge": 0, "straightEdge": 3, "branchEdge": 1, "routes": routes,
                 "bladeTipM": BLADE_TIP, "bladeLengthM": BLADE_LEN, "frog": {"sM": FROG_S, "angleDeg": FROG_ANGLE,
                 "centrelineSeparationM": sep(FROG_S)}, "foulingPointSM": foul, "flangewayM": FLANGE,
                 "timbers": TIMBERS, "branchSleepers": nb, "straightSleepers": len(straight_rects()),
                 "lever": {"sM": LEVER_S, "yM": LEVER_Y, "side": "+y, away from corner 0"},
                 "railTopM": TOP},
        lodTris=tris, skipped=False)
    print(json.dumps({"piece": piece, "tris": tris, "texel": texel}))
    return root, lods
