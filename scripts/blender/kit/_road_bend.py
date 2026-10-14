"""Shared code for the turning road centre pieces: road_bend_120_center, road_bend_60_center and
road_junction_center. `_road.py` stays byte-identical (it is a hashed dependency of the approved
road_straight pieces), so everything new lives here and reuses `_road.PROFILE` and its constants.

Joint contract. A road tile places `road_straight_edge` (x in [6, APOTHEM], rotated by k) at every
lane edge k, so a centre piece meets the edge stub at r = JOINT (6 m) on the ray through edge k's
midpoint. At that joint its vertices are exactly the stub's: PROFILE swept from
`lane_frame(k, 0.0, ROAD_WIDTH)` (the edge-0 section rotated by k, mirror-symmetric, so the sweep
direction does not matter), plus the two centre-line paint corners (+-PAINT_W / 2, PAINT_Z). No other
vertex of any LOD lies within 1 mm of a joint plane (kit_gate check_center_edge_joint, per lane).

Bends. The arm centre lines are the rays from the hex centre to the two edge midpoints, alpha =
60 * edge degrees apart. A path tangent to both rays at r = JOINT is the arc of radius
rho = JOINT * tan(alpha / 2) centred at C = (JOINT, -rho): 10.392 m for the 120-degree tile (edge 2)
and 3.464 m for the 60-degree tile (edge 1). Every LOD sweeps the same PROFILE along that arc; LODs
differ only in the number of arc segments.

Sharp bend inner side. rho = 3.464 < SINK_T, so the inner verge points (t = -4, -4.6) cannot be swept
on the bend side: they fall past C, onto the other side of both joint lines, in the corner where the
two edge stubs already overlap. The sweep stops at the kerb's outer foot (t = -3.25, 0.21 m from C);
a fan at VERGE_TOP closes the pit inside the kerb, and the dropped inner points are swept around C
on the far side (they are the stubs' own inner verge points at both joints), with their own fan.
The two fan apexes sit APEX_OFFSET from C so no vertex lands on a joint plane; the uncovered gap
between them is under 2 mm wide.

Dashes (the `_road.py` bend rule, read for fixed-period stubs). The rule spaces dashes over the whole
edge-to-edge path at period L / n, but the stubs are frozen at period P with a dash running across
the joint, on from the joint for JOINT_DASH (1.843 m). So a centre path of length L starts and ends
with that JOINT_DASH dash, and the free span F = L - 2 * JOINT_DASH holds m dashes and m + 1 gaps of
equal length F / (2m + 1), with m = round((2F / P - 1) / 2) (the count closest to P / 2 per
interval). Both ends then continue the stub pattern, and the piece is symmetric end for end.

Junction. Three arms (edges 0, 2, 4) run straight from the joint in to MOUTH (4 m) with the full
PROFILE and their stub dash (it ends at 4.157 m, before the mouth). No paint inside the box (owner
reference ref-3). The asphalt box is exact in the crown height field (distance to the nearest arm
centre line through PROFILE, flat at BED_TOP beyond the kerb line), triangulated along its crest,
valley and iso lines. Between two arms the kerb, verge and sink half-profile sweeps round a fillet of
radius RHO_F tangent to both arms' kerb lines at the mouth.
"""
import math
import os
import pathlib
import sys

import bmesh
import bpy

import numpy as np

from lib import bake, collider, export, geom, hexgeo, materials, scene, uv

import _road

JOINT = _road.CENTER_HALF
P = _road.DASH_PERIOD
JOINT_DASH = JOINT - 0.75 * P  # the stub's dash k = 1 spans [0.75 P, 1.25 P] along x
SLOT_INDEX = {name: i for i, name in enumerate(_road.SLOTS)}
POST_T = (_road.ROAD_WIDTH / 2 + _road.KERB_W + _road.ROAD_WIDTH / 2 + _road.VERGE) / 2
APEX_OFFSET = 0.002
HALF = [p for p in _road.PROFILE if p[0] >= _road.ROAD_WIDTH / 2]  # kerb foot (3, BED_TOP) out to the sink
MOUTH = 4.0
P_X = (_road.ROAD_WIDTH / 2) / math.tan(math.radians(60.0))  # kerb lines of adjacent arms meet here
RHO_F = (MOUTH - P_X) * math.tan(math.radians(60.0))


def u(deg):
    a = math.radians(deg)
    return (math.cos(a), math.sin(a))


def rot(p, k):
    x, y = hexgeo.rotate_xy((p[0], p[1]), k)
    return (x, y, p[2]) if len(p) == 3 else (x, y)


class Mesh:
    """Vertex and face lists, turned into a Blender object by `obj`. Faces carry a slot name."""

    def __init__(self):
        self.verts, self.faces, self.beds = [], [], []

    def add(self, p):
        self.verts.append((float(p[0]), float(p[1]), float(p[2])))
        return len(self.verts) - 1

    def tri_up(self, a, b, c, slot):
        """Triangle wound so its normal faces +Z."""
        pa, pb, pc = (self.verts[i] for i in (a, b, c))
        cz = (pb[0] - pa[0]) * (pc[1] - pa[1]) - (pb[1] - pa[1]) * (pc[0] - pa[0])
        assert abs(cz) > 1e-9, ("degenerate triangle", pa, pb, pc)
        self.faces.append(((a, b, c) if cz > 0 else (a, c, b), SLOT_INDEX[slot]))

    def sweep(self, rows, prof):
        """Quads between consecutive rows over every profile span i -> i + 1, wound like
        `_road.build` (spans with increasing t face +Z, walls face out). rows[s][i] are vertex
        indices for profile point prof[i]."""
        mid = len(prof) // 2
        h = [self.verts[rows[1][mid]][j] - self.verts[rows[0][mid]][j] for j in (0, 1)]
        i = next(i for i in range(len(prof) - 1) if prof[i + 1][0] > prof[i][0])
        lat = [self.verts[rows[0][i + 1]][j] - self.verts[rows[0][i]][j] for j in (0, 1)]
        flip = h[0] * lat[1] - h[1] * lat[0] < 0
        for s in range(len(rows) - 1):
            for i in range(len(prof) - 1):
                q = (rows[s][i], rows[s + 1][i], rows[s + 1][i + 1], rows[s][i + 1])
                self.faces.append((q[::-1] if flip else q, SLOT_INDEX[_road.span_slot(prof[i][0], prof[i + 1][0])]))
                if prof[i + 1][0] > prof[i][0]:
                    self.beds.append(len(self.faces) - 1)

    def extend(self, other, k=0):
        """Append `other` rotated by k edge steps."""
        base = len(self.verts)
        self.verts.extend(rot(v, k) for v in other.verts)
        n = len(self.faces)
        self.faces.extend((tuple(base + i for i in f), m) for f, m in other.faces)
        self.beds.extend(n + b for b in other.beds)

    def obj(self, name, mats, weld=False):
        bm = bmesh.new()
        vs = [bm.verts.new(v) for v in self.verts]
        faces = []
        for f, m in self.faces:
            face = bm.faces.new([vs[i] for i in f])
            face.material_index = m
            faces.append(face)
        bm.normal_update()
        bad = [faces[b].normal.z for b in self.beds if faces[b].normal.z <= 0.0]
        assert not bad, f"{name}: {len(bad)} bed faces face down"
        if weld:
            bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-4)
        bmesh.ops.triangulate(bm, faces=bm.faces)
        mesh = bpy.data.meshes.new(name)
        bm.to_mesh(mesh)
        bm.free()
        o = scene.link(bpy.data.objects.new(name, mesh))
        for slot in _road.SLOTS:
            o.data.materials.append(mats[slot])
        return o


# --- dashes -------------------------------------------------------------------------------------

def dash_intervals(length):
    """Dash intervals (arc length from the edge-0 joint) for a centre path of `length` (module doc)."""
    free = length - 2 * JOINT_DASH
    assert free > 0, length
    m = max(0, math.floor((2 * free / P - 1) / 2 + 0.5))
    w = free / (2 * m + 1)
    out = [(0.0, JOINT_DASH)]
    out += [(JOINT_DASH + w * (2 * j + 1), JOINT_DASH + w * (2 * j + 2)) for j in range(m)]
    return out + [(length - JOINT_DASH, length)]


# --- bends --------------------------------------------------------------------------------------

def bend_geometry(edge):
    alpha = 60.0 * edge
    rho = JOINT * math.tan(math.radians(alpha / 2))
    turn = 180.0 - alpha
    c = (JOINT, -rho)
    b = (c[0] + rho * u(90.0 + turn)[0], c[1] + rho * u(90.0 + turn)[1])
    want = hexgeo.rotate_xy((JOINT, 0.0), edge)
    assert abs(b[0] - want[0]) < 1e-9 and abs(b[1] - want[1]) < 1e-9, (b, want)
    return rho, turn, c


def bend_mesh(edge, segments):
    """The bend bed (no paint) at `segments` arc segments."""
    rho, turn, c = bend_geometry(edge)
    keep = [p for p in _road.PROFILE if rho + p[0] > 0.1]
    drop = [p for p in _road.PROFILE if rho + p[0] <= 0.1]
    m = Mesh()

    def at(t, z, th):
        d = u(th)
        return (c[0] + (rho + t) * d[0], c[1] + (rho + t) * d[1], z)

    thetas = [90.0 + turn * s / segments for s in range(segments + 1)]
    rows = [[m.add(at(t, z, th)) for (t, z) in keep] for th in thetas]
    m.sweep(rows, keep)
    if drop:
        # Sharp bend inner side (module doc): pit fan inside the kerb foot, far-side verge and sink.
        assert keep[0] == (-(_road.ROAD_WIDTH / 2 + _road.KERB_W), _road.VERGE_TOP), keep[0]
        assert [p[0] for p in drop] == [-_road.SINK_T, -(_road.ROAD_WIDTH / 2 + _road.VERGE)], drop
        vz = _road.VERGE_TOP
        mid = 90.0 + turn / 2
        pit = m.add((c[0] + APEX_OFFSET * u(mid)[0], c[1] + APEX_OFFSET * u(mid)[1], vz))
        far = m.add((c[0] - APEX_OFFSET * u(mid)[0], c[1] - APEX_OFFSET * u(mid)[1], vz))
        sink = [m.add(at(*drop[0], th)) for th in thetas]
        top = [m.add(at(*drop[1], th)) for th in thetas]
        for s in range(segments):
            m.tri_up(pit, rows[s][0], rows[s + 1][0], _road.VERGE_SLOT)
            m.tri_up(far, top[s], top[s + 1], _road.VERGE_SLOT)
            m.tri_up(top[s], top[s + 1], sink[s + 1], _road.VERGE_SLOT)
            m.tri_up(top[s], sink[s + 1], sink[s], _road.VERGE_SLOT)
    return m


def bend_paint(edge, segments):
    rho, turn, c = bend_geometry(edge)
    length = rho * math.radians(turn)
    seg = length / segments
    m = Mesh()
    w = _road.PAINT_W / 2
    for a, b in dash_intervals(length):
        n = max(1, math.ceil((b - a) / seg - 1e-9))
        ring = []
        for j in range(n + 1):
            th = 90.0 + math.degrees((a + (b - a) * j / n) / rho)
            ring.append([m.add((c[0] + r * u(th)[0], c[1] + r * u(th)[1], _road.PAINT_Z)) for r in (rho - w, rho + w)])
        for j in range(n):
            m.tri_up(ring[j][0], ring[j + 1][0], ring[j + 1][1], _road.PAINT)
            m.tri_up(ring[j][0], ring[j + 1][1], ring[j][1], _road.PAINT)
    return m


def bend_footprint(edge, far=20.0, samples=96):
    """Asphalt + kerb outline (|t| <= 3.25) along stub, arc and stub, for the wetness mask."""
    rho, turn, c = bend_geometry(edge)
    tk = _road.ROAD_WIDTH / 2 + _road.KERB_W
    sides = []
    for t in (tk, -tk):
        pts = [(far, t), (JOINT, t)]
        for s in range(1, samples):
            th = 90.0 + turn * s / samples
            pts.append((c[0] + (rho + t) * u(th)[0], c[1] + (rho + t) * u(th)[1]))
        pts += [rot((JOINT, -t), edge), rot((far, -t), edge)]
        sides.append(pts)
    return sides[0] + sides[1][::-1]


# --- junction -----------------------------------------------------------------------------------

def junction_unit(segments):
    """Arm 0 (joint to mouth), its asphalt box sector (|angle| <= 60) and the fillet towards arm 2,
    in the edge-0 frame. Rotated by 0, 2 and 4 edge steps it makes the whole box."""
    m = Mesh()
    rows = [[m.add((x, t, z)) for (t, z) in _road.PROFILE] for x in (MOUTH, JOINT)]
    m.sweep(rows, _road.PROFILE)
    crown = dict(_road.PROFILE[5:10])  # t -> z over the asphalt: -3, -1.5, 0, 1.5, 3
    o = m.add((0.0, 0.0, crown[0.0]))
    mouth = {t: m.add((MOUTH, t, crown[t])) for t in crown}
    for sgn in (1.0, -1.0):
        b = m.add((1.5 / math.tan(math.radians(60.0)), sgn * 1.5, crown[1.5]))
        p = m.add((P_X, sgn * 3.0, crown[3.0]))
        a = _road.ASPHALT
        m.tri_up(o, mouth[0.0], mouth[sgn * 1.5], a)
        m.tri_up(o, mouth[sgn * 1.5], b, a)
        m.tri_up(b, mouth[sgn * 1.5], mouth[sgn * 3.0], a)
        m.tri_up(b, mouth[sgn * 3.0], p, a)
    f = (MOUTH, -_road.ROAD_WIDTH / 2 - RHO_F)
    fr = []
    for s in range(segments + 1):
        d = u(90.0 + 60.0 * s / segments)
        fr.append([m.add((f[0] + (RHO_F - (t - HALF[0][0])) * d[0], f[1] + (RHO_F - (t - HALF[0][0])) * d[1], z))
                   for (t, z) in HALF])
    m.sweep(fr, HALF)
    pm = m.add((P_X, -3.0, crown[3.0]))
    for s in range(segments):
        m.tri_up(pm, fr[s][0], fr[s + 1][0], _road.ASPHALT)
    end = rot((MOUTH, _road.ROAD_WIDTH / 2), 2)
    last = m.verts[fr[-1][0]]
    assert abs(last[0] - end[0]) < 1e-9 and abs(last[1] - end[1]) < 1e-9, (last, end)
    return m


def junction_mesh(segments):
    m = Mesh()
    unit = junction_unit(segments)
    for k in (0, 2, 4):
        m.extend(unit, k)
    return m


def junction_paint(segments):
    del segments
    m = Mesh()
    w = _road.PAINT_W / 2
    for k in (0, 2, 4):
        for a, b in _road.dash_spans(MOUTH, JOINT):
            q = [m.add(rot(p, k)) for p in ((a, -w, _road.PAINT_Z), (b, -w, _road.PAINT_Z),
                                              (b, w, _road.PAINT_Z), (a, w, _road.PAINT_Z))]
            m.tri_up(q[0], q[1], q[2], _road.PAINT)
            m.tri_up(q[0], q[2], q[3], _road.PAINT)
    return m


def junction_footprint(far=20.0, samples=64):
    tk = _road.ROAD_WIDTH / 2 + _road.KERB_W
    f = (MOUTH, -_road.ROAD_WIDTH / 2 - RHO_F)
    wedge = [(far, -tk), (MOUTH, -tk)]
    for s in range(1, samples):
        d = u(90.0 + 60.0 * s / samples)
        wedge.append((f[0] + (RHO_F - _road.KERB_W) * d[0], f[1] + (RHO_F - _road.KERB_W) * d[1]))
    wedge += [rot((MOUTH, tk), 2), rot((far, tk), 2)]
    return [rot(p, k) for k in (0, 2, 4) for p in wedge]


# --- checks, masks and the piece ----------------------------------------------------------------

def joint_section_errors(obj, lanes):
    """kit_gate check_center_edge_joint in the Blender frame: at every lane k, the vertices on the
    joint plane (after rotating edge k onto edge 0) are exactly the stub's section."""
    w = _road.PAINT_W / 2
    want = sorted({(round(t * 1e4), round(z * 1e4)) for t, z in _road.PROFILE}
                  | {(round(s * w * 1e4), round(_road.PAINT_Z * 1e4)) for s in (-1, 1)})
    errs = []
    co = [obj.matrix_world @ v.co for v in obj.data.vertices]
    for k in lanes:
        sec = set()
        for v in co:
            x, y = hexgeo.rotate_xy((v.x, v.y), -k)
            if abs(x - JOINT) < 1e-3:
                sec.add((round(y * 1e4), round(v.z * 1e4)))
        if sorted(sec) != want:
            errs.append(f"{obj.name} lane {k}: section {sorted(sec)} != {want}")
    return errs


def inside(poly, xs, ys):
    """Even-odd point-in-polygon over numpy arrays."""
    out = np.zeros(xs.shape, dtype=bool)
    n = len(poly)
    for i in range(n):
        (x0, y0), (x1, y1) = poly[i], poly[(i + 1) % n]
        if y0 == y1:
            continue
        cross = (y0 > ys) != (y1 > ys)
        xi = x0 + (ys - y0) * (x1 - x0) / (y1 - y0)
        out ^= cross & (xs < xi)
    return out


def build_piece(piece, beds_fn, paint_fn, segments, lanes, footprint, generator, sockets, extra):
    """The whole centre piece, like `_road.build_piece`: three LODs, planar UV0, AO bake on the bed
    before the paint joins, masks (wetness from `footprint`), box collider, sockets, GLB, reports."""
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    root = scene.empty(piece)
    mats = {
        _road.ASPHALT: materials.pbr_material(_road.ASPHALT, materials.srgb("#3a3a3c"), 0.75),
        _road.VERGE_SLOT: materials.pbr_material(_road.VERGE_SLOT, materials.srgb("#4f7d32"), 0.9),
        _road.KERB: materials.pbr_material(_road.KERB, (*_road.KERB_COLOR, 1.0), 0.85),
        _road.PAINT: materials.pbr_material(_road.PAINT, materials.srgb("#e6e2d6"), 0.6),
    }
    beds, paints = [], []
    for i, n in enumerate(segments):
        bed = beds_fn(n).obj(f"{piece}_lod{i}_bed", mats, weld=True)
        pt = paint_fn(n).obj(f"{piece}_lod{i}_paint", mats)
        for o in (bed, pt):
            uv.planar_cap_uv(o)
        beds.append(bed)
        paints.append(pt)

    signature = bake.receiver_signature([beds[0]])
    texel = uv.measure_texel_density(beds[0], _road.MASK_SIZE)
    if not skip_bake:
        bake.setup_cycles(samples=64)
        ao = bake.new_image(f"{piece}.ao", _road.MASK_SIZE, non_color=True)
        bake.bake_to(beds[0], "AO", ao, margin=12)
        ao_png = bake.save_png(ao, str(out / f"{piece}.ao.tmp.png"))
        # Texel centres in Blender pixel order (row 0 = V 0). Porosity 0.3 on asphalt and kerb,
        # 0.7 on the verge; snow capacity 1.0 everywhere; no night emissive (as _road.build_piece).
        c = (np.arange(_road.MASK_SIZE, dtype=np.float64) + 0.5) / _road.MASK_SIZE * 2 * hexgeo.HEX_R - hexgeo.HEX_R
        xs, ys = np.meshgrid(c, c)
        wet = np.where(inside(footprint, xs, ys), 0.3, 0.7).ravel().astype(np.float32)
        bake.make_masks(_road.MASK_SIZE, 1.0, wet, 0.0, ao_png, str(out / f"{piece}.masks.png"))
        os.remove(ao_png)

    lods = []
    for i, (bed, pt) in enumerate(zip(beds, paints)):
        body = geom.join([bed, pt], f"{piece}_lod{i}")
        body.data.shade_smooth()
        body.data.set_sharp_from_angle(angle=math.radians(30.0))
        body.parent = root
        lods.append(body)
        assert [m.name for m in body.data.materials] == list(_road.SLOTS), [m.name for m in body.data.materials]
    errs = [e for o in lods for e in joint_section_errors(o, lanes)]
    assert not errs, "\n".join(errs)
    tris = [geom.tri_count(o) for o in lods]
    assert tris[0] <= 4000 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
    print(f"{piece}: tris per LOD {tris}")

    col = collider.collider_from(lods[2], "box", f"{piece}_collider")
    col.parent = root
    sockets(root)
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=signature,
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=64, marginPx=12,
                           channels={"masks": {"size": _road.MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=texel, profile=[list(p) for p in _road.PROFILE],
                           dashPeriodM=_road.DASH_PERIOD, lanes=list(lanes), skipped=skip_bake, **extra)
    return root, lods


def anchor(root, n, p):
    export.add_socket(f"anchor-prop-{n}", (p[0], p[1], _road.VERGE_TOP), root, kind="prop", radius=0.5)


def build_bend(piece, edge, segments, generator):
    rho, turn, c = bend_geometry(edge)
    length = rho * math.radians(turn)

    def sockets(root):
        # Verge sign posts at POST_T from the centre line: outer verge (and inner, where it exists).
        posts = [(rho + POST_T, 90.0 + turn / 3), (rho + POST_T, 90.0 + 2 * turn / 3)]
        if rho - POST_T > 1.0:
            posts = [(rho + POST_T, 90.0 + turn / 2), (rho - POST_T, 90.0 + turn / 2)]
        for n, (r, th) in enumerate(posts):
            anchor(root, n, (c[0] + r * u(th)[0], c[1] + r * u(th)[1]))

    return build_piece(piece, lambda n: bend_mesh(edge, n), lambda n: bend_paint(edge, n), segments,
                       (0, edge), bend_footprint(edge), generator, sockets,
                       {"bend": {"edge": edge, "radiusM": rho, "turnDeg": turn, "arcLengthM": length,
                                 "dashes": [list(d) for d in dash_intervals(length)]}})


def build_junction(piece, segments, generator):
    f = (MOUTH, -_road.ROAD_WIDTH / 2 - RHO_F)

    def sockets(root):
        r = RHO_F - (POST_T - _road.ROAD_WIDTH / 2)
        for n, k in enumerate((0, 2, 4)):
            anchor(root, n, rot((f[0] + r * u(120.0)[0], f[1] + r * u(120.0)[1]), k))

    return build_piece(piece, junction_mesh, junction_paint, segments, (0, 2, 4), junction_footprint(),
                       generator, sockets,
                       {"junction": {"mouthM": MOUTH, "filletRadiusM": RHO_F, "paint": "arm dashes stop at the mouth; none in the box"}})
