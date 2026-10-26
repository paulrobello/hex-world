"""Trimmed inner corner of the sharp bend (road_bend_60), owner A7 (2026-10-21, unblocked
2026-10-25): no overlapping verge faces at the inner corner, with the look unchanged.

The overlap. The tile placed `road_straight_edge` on lanes 0 and 1. The two stubs (x in [6, APOTHEM],
|t| <= SINK_T) overlap in a kite at the inner corner: C = (JOINT, -rho), the bend arc's centre
(rho = 6 tan 30 = 3.464 m), (JOINT, -SINK_T), (SINK_T * sqrt(3), -SINK_T) on the inner bisector at
-30 degrees, and the mirror of the second point. The centre's far-side inner fan and sink rows
(`_road_bend.bend_mesh`, around C between -90 and 30 degrees) lay inside the same kite, so three
surfaces stacked there. Everywhere in the kite the stub nearer the point (the smaller lateral offset
|t|) is on top or level with the other surfaces: the profile only falls beyond the kerb, and where
one stub is on its asphalt the other is down its sink.

The trim. Each stub keeps only its own side of the bisector, the vertical plane through the hex
centre at -30 degrees: `road_bend_60_edge_e0` (lane 0, trimmed on -y) and `road_bend_60_edge_e1`
(lane 1; in its own edge-0 frame the inner side is +y). The cut only crosses the inner verge and
sink (|t| >= rho there), never reaches the border (it meets t = -SINK_T at x = 7.967), and the
profile is level along the cut on both sides, so the two stubs meet edge to edge on the bisector.
e1 is e0 mirrored (y -> -y, faces reversed), so both carry the same cut vertices. The centre keeps
its swept arc and the pit fan inside the inner kerb, with the fan's apex exactly at C (no
APEX_OFFSET gap), and drops the far-side fan and sink rows. C is on both joint planes, so every
joint section is the stub section through C: PROFILE points with t > -rho, plus (-rho, VERGE_TOP),
plus the two paint corners (`trimmed_section`). Around C the three pieces then tile the plane:
the centre between 90 and 210 degrees, e0 between -30 and 90, e1 between 210 and 330.

The stub uses `_road.build_piece` unchanged, with `_road.build` wrapped at import time to cut every
LOD bed before the AO bake, and the centre uses `_road_bend.build_bend` with `bend_mesh` and
`joint_section_errors` replaced. `_road.py` and `_road_bend.py` stay untouched, so the approved road
pieces that import them stay current.
"""
import math

import bmesh

from lib import export, hexgeo

import _road
import _road_bend

EDGE = 1  # road_bend_60: lanes on edges 0 and 1
RHO, TURN, C = _road_bend.bend_geometry(EDGE)
HALF = math.radians(30.0 * EDGE)
# Keep n . p >= 0: edge 0's side of the inner bisector (the ray at -30 degrees in Blender XY).
KEEP_NORMAL = (math.sin(HALF), math.cos(HALF), 0.0)
MIN_TRI_AREA = 1e-4  # m^2: a bisect that lands near an existing vertex leaves a sliver


def trimmed_section(side):
    """The joint section (t, z) in 0.1 mm units of a stub trimmed on side `side` (-1: -y, +1: +y),
    paint corners included, as `_road_bend.joint_section_errors` builds its `want`."""
    w = _road.PAINT_W / 2
    pts = {(round(t * 1e4), round(z * 1e4)) for t, z in _road.PROFILE if side * t < RHO}
    pts.add((round(side * RHO * 1e4), round(_road.VERGE_TOP * 1e4)))
    pts |= {(round(s * w * 1e4), round(_road.PAINT_Z * 1e4)) for s in (-1, 1)}
    return sorted(pts)


def section_errors(obj, lanes):
    """Like `_road_bend.joint_section_errors`, against the trimmed section: lane 0 is trimmed on -y,
    and lane 1, rotated onto edge 0, on +y."""
    errs = []
    co = [obj.matrix_world @ v.co for v in obj.data.vertices]
    for k in lanes:
        side = -1 if k == 0 else 1
        sec = set()
        for v in co:
            x, y = hexgeo.rotate_xy((v.x, v.y), -k)
            if abs(x - _road_bend.JOINT) < 1e-3:
                sec.add((round(y * 1e4), round(v.z * 1e4)))
        if sorted(sec) != trimmed_section(side):
            errs.append(f"{obj.name} lane {k}: section {sorted(sec)} != {trimmed_section(side)}")
    return errs


# --- the centre ----------------------------------------------------------------------------------

def bend_mesh(edge, segments):
    """The trimmed centre bed: the arc sweep of `_road_bend.bend_mesh` (PROFILE points with
    rho + t > 0.1) and its pit fan inside the inner kerb foot, apex at C. No far-side rows."""
    assert edge == EDGE, edge
    keep = [p for p in _road.PROFILE if RHO + p[0] > 0.1]
    assert keep[0] == (-(_road.ROAD_WIDTH / 2 + _road.KERB_W), _road.VERGE_TOP), keep[0]
    m = _road_bend.Mesh()

    def at(t, z, th):
        d = _road_bend.u(th)
        return (C[0] + (RHO + t) * d[0], C[1] + (RHO + t) * d[1], z)

    thetas = [90.0 + TURN * s / segments for s in range(segments + 1)]
    rows = [[m.add(at(t, z, th)) for (t, z) in keep] for th in thetas]
    m.sweep(rows, keep)
    pit = m.add((C[0], C[1], _road.VERGE_TOP))
    for s in range(segments):
        m.tri_up(pit, rows[s][0], rows[s + 1][0], _road.VERGE_SLOT)
    return m


def build_center(piece, segments, generator):
    _road_bend.bend_mesh = bend_mesh
    _road_bend.joint_section_errors = section_errors
    return _road_bend.build_bend(piece, EDGE, segments, generator)


# --- the stubs -----------------------------------------------------------------------------------

def tri_areas(bm):
    out = []
    for f in bm.faces:
        a, b, c = (v.co for v in f.verts)
        out.append((b - a).cross(c - a).length / 2)
    return out


def trim(obj, side):
    """Cut a stub bed (edge-0 frame, full PROFILE) at the inner bisector, keeping edge 0's side,
    then mirror it for side +1. Up-facing bed faces stay up-facing."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:], dist=1e-6,
                           plane_co=(0.0, 0.0, 0.0), plane_no=KEEP_NORMAL, clear_inner=True)
    bmesh.ops.triangulate(bm, faces=bm.faces)
    off = [v.co for v in bm.verts if v.co.x * KEEP_NORMAL[0] + v.co.y * KEEP_NORMAL[1] < -1e-6]
    assert not off, (obj.name, len(off))
    if side > 0:
        for v in bm.verts:
            v.co.y = -v.co.y
        bmesh.ops.reverse_faces(bm, faces=bm.faces)
    bm.normal_update()
    small = min(tri_areas(bm))
    assert small > MIN_TRI_AREA, (obj.name, small)
    down = [f for f in bm.faces if f.normal.z < -1e-6]
    assert not down, (obj.name, len(down))
    bm.to_mesh(obj.data)
    bm.free()
    obj.data.update()


def build_stub(piece, side, segments, generator):
    """A road_straight_edge stub trimmed on side `side` (-1: the -y side, lane 0 of road_bend_60;
    +1: the +y side, lane 1), through `_road.build_piece`. Returns (root, lods)."""
    original = _road.build

    def build(name, x0, x1, n, lod, materials_by_span):
        o = original(name, x0, x1, n, lod, materials_by_span)
        trim(o, side)
        return o

    _road.build = build

    def sockets(root):
        c = hexgeo.lane_frame(0, 0.0, _road.ROAD_WIDTH)["center"]
        export.add_socket("socket-lane-0", (*c, 0.0), root, kind="road", edge=0, offset=0.0,
                          width=_road.ROAD_WIDTH)

    root, lods = _road.build_piece(piece, _road.CENTER_HALF, hexgeo.APOTHEM, segments, generator, sockets)
    errs = []
    for o in lods:
        co = [o.matrix_world @ v.co for v in o.data.vertices]
        sec = sorted({(round(v.y * 1e4), round(v.z * 1e4)) for v in co if abs(v.x - _road_bend.JOINT) < 1e-3})
        if sec != trimmed_section(side):
            errs.append(f"{o.name}: joint section {sec} != {trimmed_section(side)}")
        border = sorted({(round(v.y * 1e4), round(v.z * 1e4)) for v in co if abs(v.x - hexgeo.APOTHEM) < 1e-3})
        if border != sorted({(round(t * 1e4), round(z * 1e4)) for t, z in _road.PROFILE}):
            errs.append(f"{o.name}: border section {border} is not PROFILE")
    assert not errs, "\n".join(errs)
    return root, lods
