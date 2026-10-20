"""Shared river-spring geometry: the pinned spring bed and the lane-cut spring base (river_spring tile).

Tile `river_spring` at rotation 0: edge 0 River (lane width 8), edges 1-5 Grass. The network runs
River from Edge(0) to Node "spring" at runtime (x, z) = (-2, 0), Blender N = (-2, 0).

Pinned bed (the engine and the Rust gate use exactly this), Blender frame, z = height:
  E = (APOTHEM, 0), d = distance to the segment N..E, r = |p - N|,
  carve(w, x) = WATER + min(1.4, bed + (1.4 - bed) smoothstep(0.7 w, w + 2.5, x)),
  bed = -1.5 max(0, 1 - (x / w)^2) - 0.05,
  z = min(carve(4.33, d), carve(5.95, r)).
The river_straight channel along the stream joined to a round pool bowl of w = 5.95 (centre -2.95,
meadow height at r >= POOL_TOP = 8.45). carve(4.33, .) is `_river.profile_z`, so the edge-0 joint
and ring are river_straight_base's.

Mesh: one continuous cap (constrained Delaunay inside `_river.border_loop((0,))`) built from
- the pool: polar rings about N (with rings on the bowl's kinks 0.7 w and w), kept where the pool
  term is the lower one;
- the stream: rows of constant x carrying `_river.OFFS`, kept where the stream term is lower,
  each row closed by its point on the crease (where both terms are equal), and the crease itself
  constrained as a polyline from N, since the bed folds there;
- the meadow lattice (`relief`, at most 0.10 m deep, 0 within 2 m of the border and at the banks);
then refined: any face whose centroid misses the bed by more than the target gets its longest
non-ring edge split (exact bed height) and the cap is triangulated again. A LOD may instead carry the
stream wedge inside the crease as columns of constant y (`columns`). Every LOD is checked against
the bed at vertices and face centroids (`bed_errors`, as `_river.profile_errors`): 0.03 m where the
bed lies below y = -1.0, 0.10 m elsewhere.
"""
import math
import os
import pathlib
import sys

import bpy
import numpy as np
from mathutils import Vector
from mathutils.geometry import delaunay_2d_cdt

from lib import bake, collider, export, geom, hexgeo, lod, scene, uv

import _river
from _river import BANK_TOP, GRASS, GRASS_Z, JOINT_X, SKIRT, SLOTS, WATER, smoothstep

NODE = (-2.0, 0.0)
EAST = (hexgeo.APOTHEM, 0.0)
W_STREAM = 4.33
W_POOL = 5.95
POOL_TOP = W_POOL + 2.5  # 8.45: the bowl reaches meadow height here
POOL_KINKS = (0.7 * W_POOL, W_POOL)
WET_Z = -1.0
TOL_WET, TOL_DRY = 0.03, 0.10
RELIEF = 0.06  # under the 0.10 m contract, so interpolation keeps headroom
LANES = (0,)
MASK_SIZE = 1024


def carve(w, x):
    x = abs(x)
    if x >= w + 2.5:
        return 0.0
    b = -1.5 * max(0.0, 1.0 - (x / w) ** 2) - 0.05
    return WATER + min(1.4, b + (1.4 - b) * smoothstep(0.7 * w, w + 2.5, x))


def seg_dist(x, y):
    """Distance to the stream segment N..E."""
    t = min(max((x - NODE[0]) / (EAST[0] - NODE[0]), 0.0), 1.0)
    return math.hypot(x - (NODE[0] + t * (EAST[0] - NODE[0])), y)


def pool_r(x, y):
    return math.hypot(x - NODE[0], y - NODE[1])


def stream_z(x, y):
    return carve(W_STREAM, seg_dist(x, y))


def pool_z(x, y):
    return carve(W_POOL, pool_r(x, y))


def bed_z(x, y):
    """The pinned spring bed (Blender z)."""
    return min(stream_z(x, y), pool_z(x, y))


def relief(x, y):
    """Meadow cap swell beyond both bank tops: 0 at the border and at the bank tops (value and
    slope), at most RELIEF deep (`_river.undulation`, the same meadow as the river bases)."""
    fb = smoothstep(0.0, 2.0, hexgeo.dist_to_border(x, y))
    fc = smoothstep(BANK_TOP, BANK_TOP + 2.0, seg_dist(x, y)) * smoothstep(POOL_TOP, POOL_TOP + 2.0, pool_r(x, y))
    return -RELIEF * fb * fc * _river.undulation(x, y)


def bank_falloff(x, y):
    return smoothstep(BANK_TOP, BANK_TOP + 2.0, seg_dist(x, y)) * smoothstep(POOL_TOP, POOL_TOP + 2.0, pool_r(x, y))


# The cap's shading slope (`_river.cap_shade_slope`): `relief` without the border falloff's rise; the
# stream and the pool bowl (inside either bank top) keep their own normals.
shade_slope = _river.cap_shade_slope(RELIEF, lambda x, y: bank_falloff(x, y) * _river.undulation(x, y),
                                     lambda x, y: seg_dist(x, y) <= BANK_TOP + 1e-6 or pool_r(x, y) <= POOL_TOP + 1e-6)


def vertex_z(x, y):
    return bed_z(x, y) + relief(x, y)


def _check_bed():
    a = hexgeo.APOTHEM
    spots = (((-2.0, 0.0), -2.95), ((a, 0.0), -2.95), ((a, 4.0), -1.40), ((-2.0, 8.45), 0.0), ((-12.0, 0.0), 0.0))
    for (x, y), want in spots:
        assert abs(bed_z(x, y) - want) < 0.006, ((x, y), bed_z(x, y), want)
    for i in range(0, 801):
        d = i * 0.01
        assert abs(carve(W_STREAM, d) - _river.profile_z(d)) < 1e-12, d
    for o in _river.OFFS:  # the edge-0 joint row is river_straight's
        assert abs(bed_z(JOINT_X, o) - _river.profile_z(o)) < 1e-12, o
    e = 1e-5
    for x, y in ((-8.0, 9.0), (4.0, -10.0), (-11.5, 0.0), (3.0, 7.9)):  # border falloff 1: the true slope
        assert hexgeo.dist_to_border(x, y) >= 2.0 and shade_slope(x, y) is not None, (x, y)
        fd = ((relief(x + e, y) - relief(x - e, y)) / (2 * e), (relief(x, y + e) - relief(x, y - e)) / (2 * e))
        assert all(abs(p - q) < 1e-6 for p, q in zip(shade_slope(x, y), fd)), (x, y, fd)


_check_bed()


def crease_y(x):
    """|y| on the row x (> N.x) where the pool and stream terms are equal, or None beyond the bowl."""
    f = lambda y: pool_z(x, y) - stream_z(x, y)  # noqa: E731
    if f(BANK_TOP) >= 0.0:
        return None
    lo, hi = 0.0, BANK_TOP
    n = 400
    prev = 0.0
    for i in range(1, n + 1):
        y = BANK_TOP * i / n
        if f(y) < 0.0:
            lo, hi = prev, y
            break
        prev = y
    for _ in range(60):
        m = 0.5 * (lo + hi)
        if f(m) < 0.0:
            hi = m
        else:
            lo = m
    return 0.5 * (lo + hi)


def crease_x(o):
    """x (between N and CREASE_END_X) where the crease crosses |y| = o."""
    lo, hi = NODE[0], CREASE_END_X - 0.05
    if pool_z(hi, o) - stream_z(hi, o) < 0.0:
        return hi
    for _ in range(60):
        m = 0.5 * (lo + hi)
        if pool_z(m, o) - stream_z(m, o) < 0.0:
            lo = m
        else:
            hi = m
    return 0.5 * (lo + hi)


CREASE_END_X = NODE[0] + math.sqrt(POOL_TOP ** 2 - BANK_TOP ** 2)  # 2.97: the crease meets both bank tops


def near_joint(x, y):
    return abs(x - JOINT_X) <= 0.3 and abs(y) <= BANK_TOP + 0.3


def bed_errors(obj, strict=True):
    """Max deviation of a built surface from the pinned bed, at vertices and face centroids (as
    `_river.profile_errors`; up-facing faces), split wet (bed below WET_Z) / dry. Skirt and mouth wall excluded."""
    mesh = obj.data
    pts = [v.co.copy() for v in mesh.vertices]
    for p in mesh.polygons:
        if abs(p.normal.z) <= 0.2:
            continue
        pts.append(p.center.copy())
    wet = dry = 0.0
    for c in pts:
        if c.z < -3.5 or (abs(hexgeo.dist_to_border(c.x, c.y)) < 1e-4 and c.z < -1e-6):
            continue
        b = bed_z(c.x, c.y)
        err = abs(c.z - b)
        if b < WET_Z:
            wet = max(wet, err)
        else:
            dry = max(dry, err)
    assert not strict or (wet <= TOL_WET and dry <= TOL_DRY), f"{obj.name}: bed error wet {wet:.4f} dry {dry:.4f}"
    return {"wetMaxM": round(wet, 4), "dryMaxM": round(dry, 4)}


def _tri_errors(p):
    """Excess over target of a triangle's centroid, p = 3 (x, y, z)."""
    samples = [tuple(sum(c[k] for c in p) / 3.0 for k in range(3))]
    worst = -1.0
    for x, y, z in samples:
        b = bed_z(x, y)
        tol = 0.99 * (TOL_WET if b < WET_Z else TOL_DRY)
        worst = max(worst, abs(z - b) - tol)
    return worst


def spring_cap(name, p):
    """One LOD's cap. p: dict(rings [(radius, spokes)], row_dx, spacing, stream_rows)."""
    loop = _river.border_loop(LANES)
    pts, zs, index = [], [], {}

    def add(x, y, z=None):
        key = (round(x, 5), round(y, 5))
        if key not in index:
            index[key] = len(pts)
            pts.append((x, y))
            zs.append(vertex_z(x, y) if z is None else z)
        return index[key]

    for x, y, z in loop:
        add(x, y, z)
    nb = len(loop)
    edges = []
    # Pool: polar rings about N, where the pool term is the lower one.
    centre = add(*NODE)
    for j, (r, n) in enumerate(p["rings"]):
        for k in range(n):
            a = 2 * math.pi * (k + 0.5 * (j % 2)) / n
            x, y = NODE[0] + r * math.cos(a), NODE[1] + r * math.sin(a)
            if pool_z(x, y) <= stream_z(x, y) - 1e-4 or (r >= POOL_TOP - 1e-9 and seg_dist(x, y) >= BANK_TOP + 0.2):
                add(x, y)
    # Stream: rows of constant x up to the crease, crease constrained from N.
    xs = []
    x = NODE[0] + p["row_dx"](0.0)
    while x < CREASE_END_X - 1e-6:
        xs.append(x)
        x += p["row_dx"](x - NODE[0])
    xs.append(CREASE_END_X + 1e-3)
    xs += [CREASE_END_X + (JOINT_X - CREASE_END_X) * k / p["stream_rows"] for k in range(1, p["stream_rows"])]
    xs.append(JOINT_X)
    prev_row, prev_crease = None, (centre, centre)
    if p.get("columns"):
        # Columns instead of rows inside the crease: the stream term depends on y only, so a point
        # on the crease at each OFFS offset, joined by a constrained edge of constant y to the
        # first full row, carries the channel with far fewer vertices.
        xs = [x for x in xs if x > CREASE_END_X]
        crease = {0.0: centre}
        for o in _river.OFFS:
            if 1e-9 < abs(o) < BANK_TOP - 1e-6:
                xc = crease_x(abs(o))
                crease[round(o, 9)] = add(xc, o, bed_z(xc, o))
        ks = sorted(crease)
        pos, neg = [k for k in ks if k >= 0], [k for k in reversed(ks) if k <= 0]
        edges += [(crease[a], crease[b]) for side in (pos, neg) for a, b in zip(side, side[1:])]
        prev_row, prev_crease = crease, (crease[neg[-1]], crease[pos[-1]])
    for x in xs:
        yc = crease_y(x) if x < CREASE_END_X else None
        offs = _river.OFFS if yc is None else [o for o in _river.OFFS if abs(o) < yc - 0.12]
        row = {}
        if yc is not None:
            row[-yc] = add(x, -yc, bed_z(x, -yc))
            row[yc] = add(x, yc, bed_z(x, yc))
        for o in offs:
            row[round(o, 9)] = add(x, o, bed_z(x, o))
        ks = sorted(row)
        edges += [(row[a], row[b]) for a, b in zip(ks, ks[1:])]
        if prev_row is not None:
            edges += [(prev_row[o], row[o]) for o in sorted(row) if o in prev_row]
        if yc is not None:
            edges += [(prev_crease[0], row[-yc]), (prev_crease[1], row[yc])]
            prev_crease = (row[-yc], row[yc])
        elif prev_crease is not None and prev_crease[0] != centre:
            # The crease ends on the bank tops of the first full row.
            edges += [(prev_crease[0], row[ks[0]]), (prev_crease[1], row[ks[-1]])]
            prev_crease = None
        prev_row = row
    # Meadow lattice.
    sp = p["spacing"]
    if sp:
        for x, y in hexgeo.lattice_points(sp, sp * 0.5):
            if seg_dist(x, y) > BANK_TOP + sp * 0.5 and pool_r(x, y) > POOL_TOP + sp * 0.5 and not near_joint(x, y):
                add(x, y)
    edges = [(a, b) for a, b in edges if a != b]
    for _round in range(40):
        coords = [Vector(q) for q in pts]
        out_v, _e, out_f, orig_v, _oe, _of = delaunay_2d_cdt(coords, edges, [list(range(nb))], 1, 1e-6)
        verts = []
        for i, v in enumerate(out_v):
            if not orig_v[i]:
                raise AssertionError(f"{name}: CDT added a Steiner vertex at {tuple(v)}")
            verts.append((v.x, v.y, zs[orig_v[i][0]]))
        added = 0
        zone = [0, 0, 0]
        for f in out_f:
            tri = [verts[i] for i in f[:3]]
            if _tri_errors(tri) > 0.0:
                # Split the longest edge that is not a ring edge (keeps triangles shaped), else
                # insert the centroid.
                cand = sorted(((math.dist(tri[a][:2], tri[(a + 1) % 3][:2]), a) for a in range(3)), reverse=True)
                cx, cy = sum(t[0] for t in tri) / 3.0, sum(t[1] for t in tri) / 3.0
                for _len, a in cand:
                    if orig_v[f[a]][0] >= nb or orig_v[f[(a + 1) % 3]][0] >= nb:
                        cx = 0.5 * (tri[a][0] + tri[(a + 1) % 3][0])
                        cy = 0.5 * (tri[a][1] + tri[(a + 1) % 3][1])
                        break
                if near_joint(cx, cy) or hexgeo.dist_to_border(cx, cy) < 0.05:
                    continue
                before = len(pts)
                add(cx, cy)
                added += len(pts) - before
                zb = bed_z(cx, cy)
                zone[0 if zb < WET_Z else (1 if zb < 0 else 2)] += len(pts) - before
        print(f"{name}: round {_round}: {len(out_f)} tris, added wet/bank/meadow {zone}")
        if not added:
            break
    else:
        raise AssertionError(f"{name}: refinement did not converge")
    faces = []
    for f in out_f:
        a, b, c = (Vector(verts[i]) for i in f[:3])
        faces.append(list(f) if (b - a).cross(c - a).z > 0 else list(reversed(f)))
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate(clean_customdata=False)
    mesh.update()
    obj = scene.link(bpy.data.objects.new(name, mesh))
    for q in obj.data.polygons:
        q.use_smooth = True
    print(f"{name}: {len(verts)} verts, {len(faces)} tris, {_round} refinement rounds")
    return obj


def build_spring_base(piece, params, skirt_rows, generator):
    """The spring base: three LODs of cap + open skirt (channel span at edge 0 left to river_edge),
    masks bake, hull collider, lane socket 0 (cut = BANK_TOP), GLB and reports."""
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    root = scene.empty(piece)
    mats = _river.make_materials()
    loop = _river.border_loop(LANES)
    lods = []
    for i, (p, rows) in enumerate(zip(params, skirt_rows)):
        cap = spring_cap(f"{piece}_lod{i}_cap", p)
        geom.set_cap_normals(cap, shade_slope)
        uv.planar_cap_uv(cap)
        sk = _river.open_skirt(f"{piece}_lod{i}_skirt", loop, LANES, hexgeo.LEVEL_STEP, rows)
        sk.data.uv_layers.new(name="UVMap")
        for d in sk.data.uv_layers["UVMap"].data:
            d.uv = (0.005, 0.005)
        for o in (cap, sk):
            for name in SLOTS:
                o.data.materials.append(mats[name])
        for q in cap.data.polygons:
            q.material_index = SLOTS.index(GRASS if q.center.z >= GRASS_Z else _river.BED)
        for q in sk.data.polygons:
            q.material_index = SLOTS.index(SKIRT)
        body = geom.join([cap, sk], f"{piece}_lod{i}")
        lod.triangulate(body)
        body.parent = root
        lods.append(body)
    lod.assert_border_identical(lods)
    errs = [bed_errors(o) for o in lods]
    tris = [geom.tri_count(o) for o in lods]
    print(f"{piece}: tris per LOD {tris}, bed error per LOD {errs}")
    assert tris[0] <= 4000 and tris[1] <= 0.5 * tris[0] and tris[2] <= _river.LANE_CUT_LOD2_RATIO * tris[0], tris
    for o in lods:
        used = {o.data.materials[q.material_index].name for q in o.data.polygons}
        assert used == set(SLOTS), (o.name, used)
    col = collider.collider_from(lods[2], "hull", f"{piece}_collider")
    col.parent = root
    _river.lane_sockets(root, LANES, cut=BANK_TOP)
    texel = uv.measure_texel_density(lods[0], MASK_SIZE)
    stats = None
    if not skip_bake:
        def wet_fn(xs, ys):
            b = np.vectorize(bed_z)(xs, ys)
            return np.where(b < -1e-6, 0.9, 0.7)
        stats = _river.bake_masks(lods[0], out, piece, wet_fn=wet_fn)
        print(f"{piece}: masks over the hex footprint {stats}")
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=64, marginPx=12,
                           aoDistanceM=6.0, channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=texel, maskStats=stats, bedErrorM=errs, profileOffsets=_river.OFFS,
                           bed={"node": list(NODE), "wStream": W_STREAM, "wPool": W_POOL, "water": WATER},
                           lod2MaxRatio=_river.LANE_CUT_LOD2_RATIO, skipped=skip_bake)
    return root, lods
