"""fence_paddock: the farm's weathered grey post-and-rail paddock fence, one closed paddock laid out
in the farm tile's frame (P3 farm; owner-approved references assets-source/refs/farm ref-1 and ref-2,
approval: "Pitched barn roof; red board barn for contrast with the cottages; wheat crop. All P3 tiles
use the O4 cutaway skirt."). Scripted route, kind `prop` (LOD0 <= 1,500 tris, LOD1 <= 50 %, LOD2 <=
15 %), KTX2 class `piece`. Usage: make kit-build PIECE=fence_paddock.

Why one piece and not modular segments: the fence stands on meadow_base's relief (down to -0.25 m),
and play places every hero at y = 0 (`play.rs`, `walk.rs`), so a straight segment placed by the tile
would float or sink along its run. One piece authored in the tile frame carries the relief in its
mesh (every post foot sinks DROP below it, every rail end rides its post), draws as one primitive per
LOD (one slot), and gives the runtime one frame for the paddock polygon, the post list and the
per-run colliders.

Frame: Blender (x, y, z) exports as runtime (x, z, -y). The pivot is the hex centre at y = 0: the
farm tile places it as a hero at `at: (0, 0), yaw_deg: 0`. The outline is defined in the runtime
tile frame (x, z) and converted with B(x, z) = (x, -z).

Outline (ref-1: a fence parallel to the tile edges with a grass margin outside): the south part of
the tile, inset INSET from edges 0, 1, 2 and 3, closed on the north by the line z = Z_NORTH between
the farmyard (barn, bales) and the grazing animals. A field gate of GATE_W on the north side toward
the yard; its leaf is swung fully open and lies against the inside of the north-east run, so the
opening stays clear. Posts every <= SPAN m along each run, corner and gate posts included.

Look: one slot, `fence_paddock_timber`, on the world-space `plaster` set tinted weathered grey (the
barn's route, project.md "Budgets"); the split-rail facets carry the shape. masks is analytic (no
Cycles bake): R snow 1, G porosity 0.6, B 0, A = AO darkening toward the ground, a function of the
height above the local relief only. UV0 carries that height in V (plus a small in-plane term on
near-horizontal faces so no face has a degenerate UV), so the 64 px masks map is a vertical ramp.

LODs are rebuilt from the same layout, never decimated: LOD0 five-facet split rails (three per span),
square posts with a low pyramid top, the gate leaf with five bars and a brace; LOD1 three-facet top
and middle rails, open square posts, the leaf's stiles, two bars and the brace; LOD2 the top and
middle rails as one triangular prism per run over a flat cap (meadow_base LOD2 has no relief), and
the leaf as one prism.

Collider: one box per run (six), disjoint meshes, the north-east one widened over the open leaf. The
walker must treat each connected component as its own convex footprint (report, integration)."""
import json
import math
import os
import pathlib
import random
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import bmesh  # noqa: E402
import bpy  # noqa: E402
import numpy as np  # noqa: E402
from mathutils import Vector  # noqa: E402

from lib import bake, budgets, export, geom, hexgeo, materials, scene, uv  # noqa: E402

P = "fence_paddock"
SLOT = f"{P}_timber"
# Linear albedo of the plaster set per channel (mean of plaster.albedo.png): runtime albedo = manifest
# colour x set, so Blender stills use the product.
PLASTER_LIN = (0.703, 0.645, 0.539)
# Weathered grey timber: runtime linear albedo about (0.176, 0.158, 0.137), sRGB about (0.46, 0.44,
# 0.41), the warm grey of ref-1's rails between their sunlit tops and shaded sides. The tint is cool
# because the plaster set is warm.
COLOR = (0.25, 0.245, 0.255)
ROUGH = 0.85
MASK_SIZE = 64
SNOW, POROSITY = 1.0, 0.6

# Outline, runtime tile frame (m).
INSET = 1.6  # fence line distance from the hex border on edges 0, 1, 2 and 3
Z_NORTH = 2.5  # north run, between the yard (bales reach z = 0.5) and the animals (cow at z = 8.25)
GATE_X = (3.5, 6.5)  # gate opening on the north run (post centres): hinge post east, latch post west
SPAN = 2.6  # longest post spacing

# Members (m).
DROP = 0.3  # post feet below the relief
POST_HALF, POST_H = 0.065, 1.25
GATE_POST_HALF, GATE_POST_H = 0.08, 1.38
CAP_RISE = 0.03  # pyramid top
LEAN = 0.022  # max post lean (rad)
RAILS = (0.34, 0.71, 1.07)  # rail centre heights above the ground at each post
RAIL_JITTER = 0.025
# Five-facet split-rail section (side, up), m: a quartered log (two split faces, a rounded back in
# two facets, a narrow ridge). Max radius 0.057, inside the posts' inscribed 0.065.
RAIL5 = [(-0.036, -0.048), (0.03, -0.052), (0.046, 0.002), (0.016, 0.054), (-0.04, 0.036)]
RAIL3 = [(-0.04, -0.045), (0.045, -0.035), (0.0, 0.055)]

# Gate leaf (hinge-side frame: a along the run from the hinge post centre, w inward, z up from the
# hinge post's ground).
LEAF_OFF = 0.19  # leaf centre plane inside the fence line
LEAF_A0, LEAF_L = 0.12, 2.9
STILE_W, STILE_D = 0.09, 0.07
BARS = ((0.24, 0.075), (0.47, 0.07), (0.70, 0.07), (0.93, 0.07), (1.16, 0.085))
STILE_Z = (0.14, 1.22)
BAR_D = 0.045

HMIN, HMAX = -0.45, 1.5  # height range V spans


def falloff(x, y):
    """meadow_base.py `falloff`, copied: that generator runs on import, so it cannot be imported."""
    t = min(max(hexgeo.dist_to_border(x, y) / 2.0, 0.0), 1.0)
    return t * t * (3 - 2 * t)


def undulation(x, y):
    """meadow_base.py `undulation` (scalar path), copied."""
    s = (0.6 * math.sin(0.31 * x + 1.7) * math.cos(0.27 * y - 0.4)
         + 0.4 * math.sin(0.53 * x - 0.45 * y + 0.9))
    return 0.5 + 0.5 * s


def relief(x, y):
    """meadow_base.py `relief`, copied (Blender x, y): the cap height the fence stands on."""
    return -0.25 * falloff(x, y) * undulation(x, y)


def B(x, z):
    """Runtime tile (x, z) to Blender (x, y)."""
    return Vector((x, -z))


def outline():
    """The fence line's corners (runtime x, z), clockwise viewed from +Y starting at the latch post
    and ending at the hinge post: the gate is the open side between the last and the first."""
    a = hexgeo.APOTHEM - INSET
    zc = (a - 0.5 * a) / math.cos(math.radians(30.0))  # edge 0 line meets the edge 1 line
    apex = a / math.cos(math.radians(30.0))
    return [(GATE_X[0], Z_NORTH), (-a, Z_NORTH), (-a, zc), (0.0, apex), (a, zc), (a, Z_NORTH),
            (GATE_X[1], Z_NORTH)]


def runs():
    """[(start, end)] runtime corners of each straight run, latch post to hinge post."""
    c = outline()
    return [(c[i], c[i + 1]) for i in range(len(c) - 1)]


def posts(rng):
    """Posts along the runs: (Blender xy, ground z, half, height, lean xy, yaw, top tilt, gate)."""
    out = []
    rs = runs()
    for k, (p0, p1) in enumerate(rs):
        a, b = B(*p0), B(*p1)
        n = max(1, math.ceil((b - a).length / SPAN - 1e-9))
        for i in range(n + (1 if k == len(rs) - 1 else 0)):
            c = a.lerp(b, i / n)
            gate = (k == 0 and i == 0) or (k == len(rs) - 1 and i == n)
            d = (b - a).normalized()
            yaw = math.atan2(d.y, d.x) + rng.uniform(-0.06, 0.06)
            lean = Vector((rng.uniform(-1, 1), rng.uniform(-1, 1))) * LEAN * (0.4 if gate else 1.0)
            half = GATE_POST_HALF if gate else POST_HALF
            h = (GATE_POST_H if gate else POST_H) + rng.uniform(-0.04, 0.04)
            tilt = Vector((rng.uniform(-1, 1), rng.uniform(-1, 1))) * 0.12
            out.append({"c": c, "g": relief(c.x, c.y), "half": half, "h": h, "lean": lean, "yaw": yaw,
                        "tilt": tilt, "gate": gate, "run": k, "i": i})
    return out


def spans(ps):
    """Indices of consecutive post pairs (the chain runs latch post -> ... -> hinge post)."""
    return [(i, i + 1) for i in range(len(ps) - 1)]


def axis(p, z):
    """Point on post p's (leaning) axis at height z."""
    c = p["c"] + p["lean"] * (z - p["g"])
    return Vector((c.x, c.y, z))


class Mesh:
    """bmesh accumulator: faces oriented by an expected outward direction."""

    def __init__(self):
        self.bm = bmesh.new()

    def face(self, pts, expect):
        f = self.bm.faces.new([self.bm.verts.new(p) for p in pts])
        f.normal_update()
        if f.normal.dot(Vector(expect)) < 0:
            f.normal_flip()
        return f

    def prism(self, ring0, ring1, centre0, centre1):
        """Side faces between two equal rings (open ends), each facing away from the axis."""
        n = len(ring0)
        for i in range(n):
            j = (i + 1) % n
            mid = (ring0[i] + ring0[j] + ring1[i] + ring1[j]) / 4
            axis_mid = (centre0 + centre1) / 2
            self.face([ring0[i], ring0[j], ring1[j], ring1[i]], mid - axis_mid)

    def box(self, lo, hi, frame, skip=()):
        """A box in frame (origin, a, w, z unit vectors): lo/hi are (a, w, z) bounds."""
        o, ea, ew, ez = frame

        def P(a, w, z):
            return o + ea * a + ew * w + ez * z
        (a0, w0, z0), (a1, w1, z1) = lo, hi
        sides = {
            "-a": ([P(a0, w0, z0), P(a0, w1, z0), P(a0, w1, z1), P(a0, w0, z1)], -ea),
            "+a": ([P(a1, w0, z0), P(a1, w1, z0), P(a1, w1, z1), P(a1, w0, z1)], ea),
            "-w": ([P(a0, w0, z0), P(a1, w0, z0), P(a1, w0, z1), P(a0, w0, z1)], -ew),
            "+w": ([P(a0, w1, z0), P(a1, w1, z0), P(a1, w1, z1), P(a0, w1, z1)], ew),
            "-z": ([P(a0, w0, z0), P(a1, w0, z0), P(a1, w1, z0), P(a0, w1, z0)], -ez),
            "+z": ([P(a0, w0, z1), P(a1, w0, z1), P(a1, w1, z1), P(a0, w1, z1)], ez),
        }
        for k, (pts, n) in sides.items():
            if k not in skip:
                self.face(pts, n)

    def obj(self, name, mat):
        bmesh.ops.triangulate(self.bm, faces=self.bm.faces)
        mesh = bpy.data.meshes.new(name)
        self.bm.to_mesh(mesh)
        self.bm.free()
        o = scene.link(bpy.data.objects.new(name, mesh))
        o.data.materials.append(mat)
        return o


def post(m, p, lod):
    """A square post from DROP below the ground to its top; LOD0 has a low pyramid top, LOD1 is
    open-topped (the top is a few pixels at 150 m); no bottom."""
    c, s = math.cos(p["yaw"]), math.sin(p["yaw"])
    ea, ew = Vector((c, s, 0.0)), Vector((-s, c, 0.0))
    h = p["half"]
    foot, top = p["g"] - DROP, p["g"] + p["h"]
    corners = [(-h, -h), (h, -h), (h, h), (-h, h)]
    lo = [axis(p, foot) + (ea * u + ew * v).to_3d() for u, v in corners]
    hi = []
    for u, v in corners:
        z = top + p["tilt"].x * u + p["tilt"].y * v
        hi.append(axis(p, z) + (ea * u + ew * v).to_3d())
    centre_lo, centre_hi = axis(p, foot), axis(p, top)
    m.prism(lo, hi, centre_lo, centre_hi)
    if lod == 0:
        apex = centre_hi + Vector((0.0, 0.0, CAP_RISE))
        for i in range(4):
            j = (i + 1) % 4
            out = ((hi[i] + hi[j]) / 2 - centre_hi)
            out.z = 0.0
            m.face([hi[i], hi[j], apex], out.normalized() + Vector((0, 0, 1.5)))


def rail_ring(centre, d, prof, roll, scale):
    """The section `prof` (side, up) at `centre`, perpendicular to unit direction d, rolled and
    scaled."""
    side = Vector((-d.y, d.x, 0.0)).normalized()
    up = Vector((0.0, 0.0, 1.0))
    up = (up - d * up.dot(d)).normalized()
    cr, sr = math.cos(roll), math.sin(roll)
    out = []
    for s, u in prof:
        s2, u2 = (s * cr - u * sr) * scale, (s * sr + u * cr) * scale
        out.append(centre + side * s2 + up * u2)
    return out


def rail(m, p0, p1, h0, h1, prof, roll, scale):
    a, b = axis(p0, p0["g"] + h0), axis(p1, p1["g"] + h1)
    d = (b - a).normalized()
    m.prism(rail_ring(a, d, prof, roll, scale), rail_ring(b, d, prof, roll, scale), a, b)


def rail_heights(rng, ps):
    """Per post and rail: the height above the ground the rail meets the post at."""
    return [[h + rng.uniform(-RAIL_JITTER, RAIL_JITTER) for h in RAILS] for _ in ps]


def rail_looks(rng, n_spans):
    """Per span and rail: (roll, scale) of its split section."""
    return [[(rng.uniform(-0.45, 0.45), rng.uniform(0.88, 1.06)) for _ in RAILS] for _ in range(n_spans)]


def leaf_frame(ps):
    """(origin, a, w, z) of the open leaf: origin on the hinge post axis at its ground, a along the
    north-east run away from the gate, w toward the inside of the paddock."""
    hinge = ps[-1]
    prev = ps[-2]
    a = (prev["c"] - hinge["c"]).normalized()
    # The paddock lies south of the north run: runtime +z, Blender -y.
    w = Vector((0.0, -1.0))
    o = Vector((hinge["c"].x, hinge["c"].y, hinge["g"]))
    return o, a.to_3d(), w.to_3d(), Vector((0.0, 0.0, 1.0))


def leaf(m, ps, lod):
    """The field gate's leaf, swung open 180 degrees against the inside of the north-east run: two
    stiles, five bars (two at LOD1), a brace from the hanging stile's foot up toward the far top; at
    LOD2 one prism along it."""
    o, ea, ew, ez = leaf_frame(ps)
    frame = (o + ew * LEAF_OFF, ea, ew, ez)
    a0, a1 = LEAF_A0, LEAF_A0 + LEAF_L
    hd, hb = STILE_D / 2, BAR_D / 2
    if lod == 2:
        zc = 0.75
        c0, c1 = o + ew * LEAF_OFF + ez * zc + ea * a0, o + ew * LEAF_OFF + ez * zc + ea * a1
        prof = [(-0.03, -0.45), (0.03, -0.45), (0.0, 0.45)]
        d = (c1 - c0).normalized()
        m.prism(rail_ring(c0, d, prof, 0.0, 1.0), rail_ring(c1, d, prof, 0.0, 1.0), c0, c1)
        return
    for s0, s1 in ((a0, a0 + STILE_W), (a1 - STILE_W, a1)):
        m.box((s0, -hd, STILE_Z[0]), (s1, hd, STILE_Z[1]), frame, skip=("-z",) if lod == 0 else ("-z", "+z"))
    bars = BARS if lod == 0 else (BARS[0], BARS[-1])
    for zc, h in bars:
        m.box((a0 + STILE_W, -hb, zc - h / 2), (a1 - STILE_W, hb, zc + h / 2), frame, skip=("-a", "+a"))
    # Brace: from the hanging stile's foot (bottom bar) up to the top bar 62 % along.
    p0 = Vector((a0 + STILE_W, BARS[0][0]))
    p1 = Vector((a0 + STILE_W + 0.62 * (LEAF_L - 2 * STILE_W), BARS[-1][0]))
    d2 = (p1 - p0).normalized()
    nrm = Vector((-d2.y, d2.x)) * 0.04
    hw = hb * 0.85
    fo = frame[0]

    def Q(q, sn, sw):
        return fo + ea * (q.x + sn * nrm.x) + ez * (q.y + sn * nrm.y) + ew * (sw * hw)
    ring0 = [Q(p0, -1, -1), Q(p0, 1, -1), Q(p0, 1, 1), Q(p0, -1, 1)]
    ring1 = [Q(p1, -1, -1), Q(p1, 1, -1), Q(p1, 1, 1), Q(p1, -1, 1)]
    m.prism(ring0, ring1, fo + ea * p0.x + ez * p0.y, fo + ea * p1.x + ez * p1.y)


def build(lod, ps, heights, looks):
    m = Mesh()
    sp = spans(ps)
    if lod < 2:
        for p in ps:
            post(m, p, lod)
        for k, (i0, i1) in enumerate(sp):
            rails = range(3) if lod == 0 else (1, 2)
            for r in rails:
                roll, scale = looks[k][r]
                rail(m, ps[i0], ps[i1], heights[i0][r], heights[i1][r], RAIL5 if lod == 0 else RAIL3, roll, scale)
    else:
        # One prism per run for the top and middle rails, over a flat cap (meadow_base LOD2 is flat).
        for (q0, q1) in runs():
            a, b = B(*q0), B(*q1)
            d = (b - a).normalized().to_3d()
            for h in RAILS[1:]:
                c0, c1 = a.to_3d() + Vector((0, 0, h)), b.to_3d() + Vector((0, 0, h))
                m.prism(rail_ring(c0, d, RAIL3, 0.0, 1.15), rail_ring(c1, d, RAIL3, 0.0, 1.15), c0, c1)
    leaf(m, ps, lod)
    return m


def height_v(h):
    return 0.04 + 0.92 * min(max((h - HMIN) / (HMAX - HMIN), 0.0), 1.0)


def set_uv(obj, flat):
    """UV0 per face: V carries the height above the ground (meadow relief, or 0 on the flat LOD2), U
    a horizontal in-plane coordinate; near-horizontal faces add a small in-plane V term so no face's
    UV is degenerate (MikkTSpace tangents). U stays in [0, 1] over the tile."""
    me = obj.data
    uvl = me.uv_layers.get("UVMap") or me.uv_layers.new(name="UVMap")
    for poly in me.polygons:
        n = poly.normal
        nh = Vector((n.x, n.y, 0.0))
        if nh.length > 0.3:
            e1 = Vector((-nh.y, nh.x, 0.0)).normalized()
            w = nh.normalized()
        else:
            e1, w = Vector((1.0, 0.0, 0.0)), Vector((0.0, 1.0, 0.0))
        c = poly.center
        for li in poly.loop_indices:
            p = me.vertices[me.loops[li].vertex_index].co
            g = 0.0 if flat else relief(p.x, p.y)
            u = 0.5 + 0.5 * p.to_2d().dot(e1.to_2d()) / hexgeo.HEX_R
            v = height_v(p.z - g) + 0.02 * (p - c).dot(w)
            uvl.data[li].uv = (u, v)


def uv_min_area(obj):
    me = obj.data
    uvl = me.uv_layers["UVMap"].data
    worst = 1.0
    for poly in me.polygons:
        q = [uvl[li].uv for li in poly.loop_indices]
        a = 0.0
        for i in range(1, len(q) - 1):
            e1, e2 = q[i] - q[0], q[i + 1] - q[0]
            a += abs(e1.x * e2.y - e1.y * e2.x) / 2.0
        worst = min(worst, a / max(poly.area, 1e-12))
    return worst


def ao(h):
    """masks.A by height above the ground: the grass and the ground occlude the post feet and the
    bottom rail; full sky from about 0.6 m up."""
    t = min(max((h + 0.05) / 0.65, 0.0), 1.0)
    return 0.58 + 0.42 * t * t * (3 - 2 * t)


def masks_png(path):
    """R snow, G porosity, B 0, A = ao(height) along V (Blender pixel rows run bottom to top)."""
    n = MASK_SIZE
    rows = (np.arange(n) + 0.5) / n
    h = HMIN + (rows - 0.04) / 0.92 * (HMAX - HMIN)
    a = np.array([ao(x) for x in h], np.float32)
    px = np.zeros((n, n, 4), np.float32)
    px[..., 0], px[..., 1], px[..., 2] = SNOW, POROSITY, 0.0
    px[..., 3] = a[:, None]
    bake.write_png(n, px.reshape(-1, 4), path)
    return {"aoMin": float(a.min()), "aoMax": float(a.max()), "aoAtGround": ao(0.0), "aoAtRails": [ao(r) for r in RAILS]}


def colliders(ps, root):
    """One box per run: from DROP below its lowest ground to above the tallest post, POST + margin
    wide; the north-east run's box also covers the open leaf. Disjoint meshes, flat-shaded, with an
    oblique planar UV (no zero tangents). Returns the runtime footprints."""
    boxes, feet = [], []
    rs = runs()
    for k, (q0, q1) in enumerate(rs):
        a, b = B(*q0), B(*q1)
        d = (b - a).normalized()
        n = Vector((-d.y, d.x))
        mine = [p for p in ps if p["run"] == k or (p["run"] == k + 1 and p["i"] == 0)]
        lo = min(p["g"] for p in mine) - DROP
        hi = max(p["g"] + p["h"] for p in mine) + 0.05
        w0, w1 = -0.12, 0.12
        if k == len(rs) - 1:
            # The leaf lies inside the paddock (Blender -y) along this run.
            inside = Vector((0.0, -1.0))
            ext = LEAF_OFF + STILE_D / 2 + 0.06
            if n.dot(inside) > 0:
                w1 = ext
            else:
                w0 = -ext
        e0, e1 = a - d * 0.12, b + d * 0.12
        corners = [e0 + n * w0, e1 + n * w0, e1 + n * w1, e0 + n * w1]
        boxes.append((corners, lo, hi))
        feet.append([[round(c.x, 3), round(-c.y, 3)] for c in corners])
    bm = bmesh.new()
    for corners, lo, hi in boxes:
        vs = [bm.verts.new((c.x, c.y, z)) for z in (lo, hi) for c in corners]
        faces = [(0, 1, 2, 3), (4, 7, 6, 5), (0, 4, 5, 1), (1, 5, 6, 2), (2, 6, 7, 3), (3, 7, 4, 0)]
        for f in faces:
            bm.faces.new([vs[i] for i in f])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bmesh.ops.triangulate(bm, faces=bm.faces)
    me = bpy.data.meshes.new(f"{P}_collider")
    bm.to_mesh(me)
    bm.free()
    col = scene.link(bpy.data.objects.new(f"{P}_collider", me))
    col["collider"] = "boxes"
    col.data.materials.append(bpy.data.materials[SLOT])
    cuv = col.data.uv_layers.new(name="UVMap")
    for poly in col.data.polygons:
        poly.use_smooth = False
    for loop in col.data.loops:
        q = col.data.vertices[loop.vertex_index].co
        cuv.data[loop.index].uv = (0.031 * q.x + 0.017 * q.y + 0.07 * q.z + 0.5,
                                   0.011 * q.x - 0.023 * q.y + 0.29 * q.z + 0.5)
    col.parent = root
    return feet


def paddock_polygon(inset=0.5):
    """The grazing polygon (runtime x, z) inside the fence line by `inset`, the gate side closed:
    the north side west to east, then the east side, the south apex and the west side."""
    a = hexgeo.APOTHEM - INSET - inset
    zc = (a - 0.5 * a) / math.cos(math.radians(30.0))
    apex = a / math.cos(math.radians(30.0))
    zn = Z_NORTH + inset
    zc = max(zc, zn)
    return [[round(x, 3), round(z, 3)] for x, z in ((-a, zn), (a, zn), (a, zc), (0.0, apex), (-a, zc))]


def main():
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    out.mkdir(parents=True, exist_ok=True)
    scene.reset_scene(P)
    root = scene.empty(P)
    mat = materials.pbr_material(SLOT, (*(c * s for c, s in zip(COLOR, PLASTER_LIN)), 1.0), ROUGH)
    rng = random.Random(P)
    ps = posts(rng)
    heights = rail_heights(rng, ps)
    looks = rail_looks(rng, len(ps) - 1)

    # Clearances the layout promises: posts inside the border by at least INSET - half, the leaf
    # bottom above the ground along its length, the bottom rail above the ground mid-span.
    border = min(hexgeo.dist_to_border(p["c"].x, p["c"].y) for p in ps)
    lo_, la, lw, _lz = leaf_frame(ps)
    leaf_clear = min(lo_.z + STILE_Z[0] - relief(*(lo_ + lw * LEAF_OFF + la * t).to_2d())
                     for t in np.linspace(LEAF_A0, LEAF_A0 + LEAF_L, 30))
    rail_clear = 9.0
    for i0, i1 in spans(ps):
        p0, p1 = ps[i0], ps[i1]
        for t in np.linspace(0, 1, 9):
            c = p0["c"].lerp(p1["c"], t)
            z = (p0["g"] + heights[i0][0]) * (1 - t) + (p1["g"] + heights[i1][0]) * t
            rail_clear = min(rail_clear, z - 0.055 - relief(c.x, c.y))
    print(f"{P}: {len(ps)} posts, {len(ps) - 1} spans, border clearance {border:.3f} m, "
          f"leaf bottom clearance {leaf_clear:.3f} m, bottom rail clearance {rail_clear:.3f} m")
    assert border >= INSET - 1e-6, border
    assert leaf_clear >= 0.05, leaf_clear
    assert rail_clear >= 0.2, rail_clear

    lods = [build(i, ps, heights, looks).obj(f"{P}_lod{i}", mat) for i in range(3)]
    tris = tuple(geom.tri_count(o) for o in lods)
    print(f"{P}: tris per LOD {tris}, LOD1/LOD2 ceilings {budgets.lod_targets(tris[0], 'prop')}")
    budgets.check(tris, "prop", P)
    for i, o in enumerate(lods):
        set_uv(o, flat=i == 2)
        worst = uv_min_area(o)
        print(f"{P}: lod{i} min UV area per m² {worst:.3e}")
        assert worst > 1e-6, (o.name, worst)
        o.data.shade_flat()
        o.parent = root
    signature = bake.receiver_signature(lods)
    density = uv.measure_texel_density(lods[0], MASK_SIZE)
    mstats = masks_png(str(out / f"{P}.masks.png"))
    feet = colliders(ps, root)
    export.export_glb(root, str(out / f"{P}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, P, __file__)

    def rt(v):
        return [round(v.x, 3), round(-v.y, 3)]
    post_list = [{"at": rt(p["c"]), "ground": round(p["g"], 3), "top": round(p["g"] + p["h"], 3),
                  "half": p["half"], "gate": p["gate"]} for p in ps]
    leaf_o = lo_ + lw * LEAF_OFF
    layout = {
        "frame": "runtime tile frame (x, z), hero at (0, 0) yaw 0, heights y",
        "fenceLine": [list(c) for c in outline()],
        "gate": {"opening": [[GATE_X[0], Z_NORTH], [GATE_X[1], Z_NORTH]], "widthM": GATE_X[1] - GATE_X[0],
                 "leaf": [rt(leaf_o + la * LEAF_A0), rt(leaf_o + la * (LEAF_A0 + LEAF_L))],
                 "leafState": "open 180 degrees, against the inside of the north-east run"},
        "paddockPolygon": paddock_polygon(),
        "colliderBoxes": feet,
        "posts": post_list,
    }
    bake.write_bake_report(
        str(out / "bake-report.json"), receiverSignature=signature, device="none", samples=0, marginPx=0,
        channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}}, texelDensityPxPerM=density,
        reason="no Cycles bake: masks is an analytic ramp in V (height above the ground)",
        masks={"snow": SNOW, "porosity": POROSITY, "night": 0.0, **mstats}, lodTris=list(tris),
        layout=layout, skipped=os.environ.get("HEXW_SKIP_BAKE") == "1")
    print(json.dumps({"piece": P, "tris": tris, "posts": len(ps), "paddock": layout["paddockPolygon"]}))


main()
