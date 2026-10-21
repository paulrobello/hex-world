"""The tight river bend across adjacent edges (river_bend_60: River lanes on edges 0 and 1), reusing the
approved river family through module globals: this file registers the layout `bend60` in
`_river.LAYOUTS` for `_river.build_base` (profile, notched ring, skirt, masks bake, sockets,
reports); the base also imports `_river_flat` (the flattened meadow relief, owner D6-Q1), and the
dressing goes through `_river_bend_60_dress.py`, so neither piece hashes the other's helper.

Centre line (Blender frame, Z up, edge 0 faces +X): the river_edge stub owns the last STUB_L metres
of each lane and is straight, so the bend is a straight 1 m stub from each edge midpoint in to the
joint plane, joined by the arc tangent to both stubs at the joints: radius R60 = JOINT_X tan 30 deg
= 7.4226 m about C60 = (JOINT_X, -R60) (runtime (12.856, 7.423)), sweeping 120 degrees from A0 =
90 deg (the edge-0 joint) to A1 = 210 deg (the edge-1 joint) through the tile side of C60. The bed
profile (`_river.profile_z`) is exact on this composite line everywhere, stubs included; the
corner-centred 8 m arc would sit 0.063 m off the stub at the joints, about 0.06 m of height at the
waterline (twice the contract's 0.03 m). The tile's network is this line as a polyline sampled every
ARC_STEP_DEG (`network_points`, chord sag 0.0018 m), which the water adapter and hex-sim read as
given (`river_segments`, `segment_shape`).

Inner knob: the inner bank top is the circle of BANK_TOP_IN = R60 - BANK_TOP = 0.59 m about C60, so
radial rows crowd there. Each interior offset o carries a row step (`steps`): it sits on row s only
where s is a multiple of its step, chosen from its radius so that vertices along it stay at least
MIN_ALONG apart and its chord sag stays inside SAG_BUDGET. `_river.channel_cap` constrains an offset
between neighbouring rows only where both carry it, so a stepped offset is left to the CDT between
the rows that carry it; `_river.profile_errors` checks every LOD's vertices and face centroids.
"""
import math

import _river
from lib import hexgeo

R60 = _river.JOINT_X * math.tan(math.radians(30.0))  # 7.4226
C60 = (_river.JOINT_X, -R60)
A0, A1 = math.radians(90.0), math.radians(210.0)
BANK_TOP_IN = R60 - _river.BANK_TOP  # 0.59: radius of the inner bank top about C60
MID0 = (hexgeo.APOTHEM, 0.0)
J0 = (_river.JOINT_X, 0.0)
MID1 = hexgeo.rotate_xy(MID0, 1)
J1 = hexgeo.rotate_xy(J0, 1)
ARC_STEP_DEG = 2.5
MIN_ALONG = 0.22  # m between neighbouring vertices of one offset along the arc
SAG_BUDGET = 0.012  # m of height from chord sag per offset (the rest of 0.03 is radial interpolation)


def _seg_dist(x, y, a, b):
    ax, ay = a
    dx, dy = b[0] - ax, b[1] - ay
    t = max(0.0, min(1.0, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)))
    return math.hypot(x - ax - t * dx, y - ay - t * dy)


def _arc_dist(x, y):
    a = math.atan2(y - C60[1], x - C60[0]) % (2.0 * math.pi)
    if A0 <= a <= A1:
        return abs(math.hypot(x - C60[0], y - C60[1]) - R60)
    return min(math.hypot(x - J0[0], y - J0[1]), math.hypot(x - J1[0], y - J1[1]))


def dist_bend60(x, y):
    """Distance to the composite centre line: edge-0 stub, the 120 degree arc, edge-1 stub."""
    return min(_seg_dist(x, y, MID0, J0), _arc_dist(x, y), _seg_dist(x, y, J1, MID1))


def _slope(o):
    e = 1e-4
    return abs(_river.profile_z(o + e) - _river.profile_z(o - e)) / (2 * e)


def steps(offsets, n, min_along=MIN_ALONG):
    """Row step per offset for n segments: the largest power of two dividing n with the offset's
    vertices at least min_along apart along the arc and its chord sag x profile slope at most
    SAG_BUDGET. Offsets on the outer side and down the channel get 1."""
    dth = (A1 - A0) / n
    out = []
    for o in offsets:
        rho = R60 + o
        st = 1
        while n % (2 * st) == 0:
            if rho * st * dth >= min_along:
                break
            if rho * (2 * st * dth) ** 2 / 8.0 * _slope(o) > SAG_BUDGET:
                break
            st *= 2
        out.append((o, st))
    return out


def bend60_rows(n, inner=None):
    """Rows of the tight bend as lists of (x, y, offset): the straight joint rows at both ends (OFFS,
    the seam; each is the arc's radial row at its end, the stubs being tangent), radial rows of the
    7.42 m arc between them. `inner` is a list of (offset, step); default OFFS_FINE with `steps`.
    Radial offset o maps to Blender y = o at edge 0 (outer side +)."""
    inner = steps(_river.OFFS_FINE, n) if inner is None else inner
    cx, cy = C60
    rows = [[(x, y, o) for (x, y), o in zip(_river.joint_row(0), _river.OFFS)]]
    for s in range(1, n):
        a = A0 + (A1 - A0) * s / n
        rows.append([(cx + (R60 + o) * math.cos(a), cy + (R60 + o) * math.sin(a), o)
                     for o, st in inner if s % st == 0])
    rows.append([(x, y, o) for (x, y), o in zip(_river.joint_row(1, flip=True), _river.OFFS)])
    return rows


_river.LAYOUTS["bend60"] = {"lanes": (0, 1), "dist": dist_bend60, "rows": bend60_rows}

_profile_errors = _river.profile_errors


def profile_errors(obj, layout, strict=True, **tol):
    """`_river.profile_errors`, printing each LOD's numbers and triangle count before its assert
    (`build_base` asserts the ratios only after measuring every LOD)."""
    from lib import geom
    dist = _river.LAYOUTS[layout]["dist"]
    worst = (0.0, None)
    pts = [v.co for v in obj.data.vertices if v.co.z > -_river.LEVEL_EPS_SKIRT]
    pts += [p.center for p in obj.data.polygons if abs(p.normal.z) > 0.2]
    for c in pts:
        if abs(hexgeo.dist_to_border(c.x, c.y)) < 1e-4 and c.z < -1e-6:
            continue
        d = dist(c.x, c.y)
        e = abs(c.z - _river.profile_z(d))
        if _river.profile_z(d) < _river.WET_Z and e > worst[0]:
            rho = math.hypot(c.x - C60[0], c.y - C60[1])
            worst = (round(e, 4), (round(c.x, 2), round(c.y, 2), round(d, 3), round(rho, 3)))
    print(f"{obj.name}: tris {geom.tri_count(obj)}, profile error {_profile_errors(obj, layout, False, **tol)}, "
          f"worst wet (err, (x, y, d, rho)) {worst}")
    return _profile_errors(obj, layout, strict, **tol)


_river.profile_errors = profile_errors

def network_points(step_deg=ARC_STEP_DEG):
    """The tile network's interior points in runtime (x, z) = (Blender x, -Blender y): the two joints
    and the arc between them every step_deg (the edge midpoints are the network's own ends)."""
    n = round(120.0 / step_deg)
    pts = [(C60[0] + R60 * math.cos(A0 + (A1 - A0) * i / n), C60[1] + R60 * math.sin(A0 + (A1 - A0) * i / n))
           for i in range(n + 1)]
    return [(round(x, 3), round(-y, 3)) for x, y in pts]


def _check():
    """Joint rows are the arc's end rows (tangent stubs); the stubs read straight; the corner and the
    inner knob read as meadow; a point on the arc reads 0; the profile is continuous across both
    joints."""
    for row, a in ((_river.joint_row(0), A0), (_river.joint_row(1, flip=True), A1)):
        for (x, y), o in zip(row, _river.OFFS):
            ex, ey = C60[0] + (R60 + o) * math.cos(a), C60[1] + (R60 + o) * math.sin(a)
            assert math.hypot(x - ex, y - ey) < 1e-9, (x, y, ex, ey)
    for o in (-6.0, -2.5, 0.0, 3.0, 6.5):
        assert abs(dist_bend60(hexgeo.APOTHEM - 0.5, o) - abs(o)) < 1e-9, o
        x, y = hexgeo.rotate_xy((hexgeo.APOTHEM - 0.5, o), 1)
        assert abs(dist_bend60(x, y) - abs(o)) < 1e-9, o
    corner = hexgeo.rotate_xy((hexgeo.APOTHEM, -8.0), 0)
    assert abs(dist_bend60(*corner) - 8.0) < 1e-9, dist_bend60(*corner)
    assert dist_bend60(*C60) == R60
    mid = (C60[0] + R60 * math.cos(math.radians(150)), C60[1] + R60 * math.sin(math.radians(150)))
    assert dist_bend60(*mid) < 1e-9
    assert abs(BANK_TOP_IN - 0.5926) < 1e-3 and abs(R60 - 7.4226) < 1e-3
    assert abs(math.hypot(J1[0] - C60[0], J1[1] - C60[1]) - R60) < 1e-9


_check()
