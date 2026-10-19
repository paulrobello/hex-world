"""Shared code for the single-track rail centre pieces: rail_straight_center and
rail_bend_60_center (P3, refs `assets-source/refs/rail`, owner A17: slightly wider than standard
gauge, single track, bends across adjacent edges).

Edge contract. Unlike the roads, a rail piece runs from edge midpoint to edge midpoint with no edge
stub: a road-style joint at r = 6 would leave the 60-degree bend a 3.46 m radius, which no train can
take. So the rail meets the hex border itself, square-on, with its lane centre on the edge midpoint
(`hexgeo.lane_frame(k, 0.0, LANE_WIDTH)`), and the cross-section there is SECTION: the ballast bed
plus both rails, mirror-symmetric about the lane centre and identical in every LOD and in every rail
piece, so any two rail pieces meet vertex for vertex across an edge (`border_section_errors`).
Sleepers stop short of the border (they sit at (j + 0.5) of the spacing), so they never touch it.

- Straight: x in [-APOTHEM, APOTHEM], lanes on edges 0 and 3.
- Bend: an arc of radius BEND_R = 8 m centred on corner 0 (Blender (APOTHEM, -8)), from the edge-0
  midpoint (angle 90 degrees about the corner) to the edge-1 midpoint (210 degrees). Both edges pass
  through the corner, so the arc meets each border square-on; the track turns 120 degrees over an
  arc length of 16.755 m.

Cross-section (t across the track, z above the cap ring at z = 0): ballast top at BALLAST_TOP out to
the shoulder, a slope to the toe at TOE_Z, then a sink to SINK_Z that dives under the meadow cap
(relief in [-0.25, 0]) like the road verge. Gauge GAUGE between the rail heads' inner faces.

Masks are analytic (no Cycles bake): the planar UV0 maps the 32 m square, and R snow, G porosity and
A AO are a field of (along, across) per texel, with the cribs between sleepers darkened and the
sleeper and rail footprints at A = 1, so stacked surfaces under the planar UV never share a baked
value that belongs to another surface.
"""
import json
import math
import os
import pathlib
import sys

import bmesh
import bpy

import numpy as np

from lib import bake, budgets, collider, export, geom, hexgeo, materials, scene, uv

GAUGE = 1.6
HEAD_W = 0.08
RAIL_T = GAUGE / 2 + HEAD_W / 2  # rail centre offset from the lane centre
BALLAST_TOP, SHOULDER_T = 0.22, 1.75
TOE_T, TOE_Z = 2.5, 0.03
SINK_T, SINK_Z = 2.9, -0.30
LANE_WIDTH = 2 * SHOULDER_T  # the socket `width`: the ballast top between the shoulders
SLEEPER_HALF_L, SLEEPER_HALF_W = 1.3, 0.13
SLEEPER_BOTTOM, SLEEPER_TOP = 0.18, 0.30
SLEEPER_PITCH = 0.7
RAIL_TOP = SLEEPER_TOP + 0.16
WEB_HALF = 0.022
BEND_R = 8.0
BEND_C = (hexgeo.APOTHEM, -8.0)
BEND_FROM, BEND_TURN = 90.0, 120.0
UV_LIFT = 0.002  # UV units per metre of height, so vertical faces keep a non-degenerate UV
MASK_SIZE = 512  # 16 px/m over the 32 m square, as the roads

BALLAST, SLEEPER, STEEL, HEAD = "rail_ballast", "rail_sleeper", "rail_steel", "rail_head"
SLOTS = (BALLAST, SLEEPER, STEEL, HEAD)
SLOT_INDEX = {s: i for i, s in enumerate(SLOTS)}
# Linear colours, equal to the manifest slots, so Blender renders show the runtime colours.
SLEEPER_COLOR, SLEEPER_ROUGH = (0.075, 0.056, 0.042), 0.85
STEEL_COLOR, STEEL_ROUGH = (0.13, 0.085, 0.06), 0.6
HEAD_COLOR, HEAD_ROUGH = (0.56, 0.56, 0.54), 0.28
BALLAST_TINT = (0.8, 0.9, 1.2)  # on the gravel set (linear mean 0.256, 0.224, 0.175): mid grey

BALLAST_PROFILE = [(-SINK_T, SINK_Z), (-TOE_T, TOE_Z), (-SHOULDER_T, BALLAST_TOP),
                   (SHOULDER_T, BALLAST_TOP), (TOE_T, TOE_Z), (SINK_T, SINK_Z)]
RAIL_PROFILE = [(-WEB_HALF, SLEEPER_TOP), (-WEB_HALF, RAIL_TOP - 0.05), (-HEAD_W / 2, RAIL_TOP - 0.035),
                (-HEAD_W / 2, RAIL_TOP), (HEAD_W / 2, RAIL_TOP), (HEAD_W / 2, RAIL_TOP - 0.035),
                (WEB_HALF, RAIL_TOP - 0.05), (WEB_HALF, SLEEPER_TOP)]


def rail_profile(side):
    return [(side * RAIL_T + t, z) for t, z in RAIL_PROFILE]


def section():
    """The border cross-section every rail piece shares, in 0.1 mm units, sorted."""
    pts = set()
    for prof in (BALLAST_PROFILE, rail_profile(-1), rail_profile(1)):
        pts |= {(round(t * 1e4), round(z * 1e4)) for t, z in prof}
    return sorted(pts)


def u(deg):
    a = math.radians(deg)
    return (math.cos(a), math.sin(a))


class Mesh:
    """Vertex and face lists; each face is wound to face its `want` direction."""

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

    def sweep(self, frames, prof, slot_of):
        """Quads between consecutive frames over each profile span. frames: (origin xy, lateral xy);
        profile points (t, z) sit at origin + t * lateral. Spans face (-dz) * lateral + dt * up."""
        rows = [[self.add((o[0] + t * l[0], o[1] + t * l[1], z)) for t, z in prof] for o, l in frames]
        for s in range(len(frames) - 1):
            lat = [(frames[s][1][j] + frames[s + 1][1][j]) / 2 for j in (0, 1)]
            for i in range(len(prof) - 1):
                dt, dz = prof[i + 1][0] - prof[i][0], prof[i + 1][1] - prof[i][1]
                want = (-dz * lat[0], -dz * lat[1], dt)
                self.face((rows[s][i], rows[s + 1][i], rows[s + 1][i + 1], rows[s][i + 1]),
                          slot_of(prof[i], prof[i + 1]), want)

    def sleeper(self, origin, lateral, tangent, top_only):
        o, l, d = origin, lateral, tangent

        def at(a, b, z):
            return self.add((o[0] + a * SLEEPER_HALF_L * l[0] + b * SLEEPER_HALF_W * d[0],
                             o[1] + a * SLEEPER_HALF_L * l[1] + b * SLEEPER_HALF_W * d[1], z))

        top = {(a, b): at(a, b, SLEEPER_TOP) for a in (-1, 1) for b in (-1, 1)}
        self.face((top[-1, -1], top[1, -1], top[1, 1], top[-1, 1]), SLEEPER, (0, 0, 1))
        if top_only:
            return
        bot = {(a, b): at(a, b, SLEEPER_BOTTOM) for a in (-1, 1) for b in (-1, 1)}
        for (a0, b0), (a1, b1), w in (((-1, -1), (-1, 1), (-l[0], -l[1], 0)), ((1, -1), (1, 1), (l[0], l[1], 0)),
                                       ((-1, -1), (1, -1), (-d[0], -d[1], 0)), ((-1, 1), (1, 1), (d[0], d[1], 0))):
            self.face((bot[a0, b0], bot[a1, b1], top[a1, b1], top[a0, b0]), SLEEPER, w)

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


def rail_slot(a, b):
    return HEAD if a[1] == b[1] == RAIL_TOP else STEEL


def track(frames, sleepers, top_only):
    """Ballast, both rails along `frames`, and a sleeper at each (origin, lateral, tangent)."""
    m = Mesh()
    m.sweep(frames, BALLAST_PROFILE, lambda a, b: BALLAST)
    for side in (-1, 1):
        m.sweep(frames, rail_profile(side), rail_slot)
    for o, l, d in sleepers or ():
        m.sleeper(o, l, d, top_only)
    return m


# --- the two paths ------------------------------------------------------------------------------

def straight_frames(n):
    a = hexgeo.APOTHEM
    return [((-a + 2 * a * s / n, 0.0), (0.0, 1.0)) for s in range(n + 1)]


def straight_sleepers():
    a = hexgeo.APOTHEM
    n = round(2 * a / SLEEPER_PITCH)
    return [((-a + 2 * a * (j + 0.5) / n, 0.0), (0.0, 1.0), (1.0, 0.0)) for j in range(n)]


def bend_point(theta, r=BEND_R):
    d = u(theta)
    return (BEND_C[0] + r * d[0], BEND_C[1] + r * d[1])


def bend_frames(n):
    return [(bend_point(th), u(th)) for th in (BEND_FROM + BEND_TURN * s / n for s in range(n + 1))]


def bend_sleepers():
    n = round(BEND_R * math.radians(BEND_TURN) / SLEEPER_PITCH)
    out = []
    for j in range(n):
        th = BEND_FROM + BEND_TURN * (j + 0.5) / n
        out.append((bend_point(th), u(th), u(th + 90.0)))
    return out


def sleeper_angles_or_x(kind):
    """Sleeper centres: x for the straight, angle about the corner (degrees) for the bend."""
    if kind == "straight":
        return [o[0] for o, _, _ in straight_sleepers()]
    n = len(bend_sleepers())
    return [BEND_FROM + BEND_TURN * (j + 0.5) / n for j in range(n)]


# --- checks -------------------------------------------------------------------------------------

def border_section(obj, k):
    """(t, z) of every vertex on edge k's border plane, rotated onto edge 0, in 0.1 mm units."""
    sec = set()
    for v in obj.data.vertices:
        co = obj.matrix_world @ v.co
        x, y = hexgeo.rotate_xy((co.x, co.y), -k)
        if abs(x - hexgeo.APOTHEM) < 1e-3:
            sec.add((round(y * 1e4), round(co.z * 1e4)))
    return sorted(sec)


def border_section_errors(obj, lanes):
    want = section()
    errs = []
    for k in lanes:
        got = border_section(obj, k)
        if got != want:
            errs.append(f"{obj.name} edge {k}: section {got} != {want}")
    return errs


def interior_errors(obj, lanes):
    """No vertex may sit outside the hex, and none on a non-lane border."""
    errs = []
    for v in obj.data.vertices:
        co = obj.matrix_world @ v.co
        d = hexgeo.dist_to_border(co.x, co.y)
        if d < -1e-4:
            errs.append(f"{obj.name}: vertex {tuple(co)} outside the hex")
        elif d < 1e-3 and not any(abs(hexgeo.rotate_xy((co.x, co.y), -k)[1]) <= SINK_T + 1e-3 and
                                  abs(hexgeo.rotate_xy((co.x, co.y), -k)[0] - hexgeo.APOTHEM) < 1e-3
                                  for k in lanes):
            errs.append(f"{obj.name}: vertex {tuple(co)} on a border with no lane")
    return errs[:5]


# --- masks --------------------------------------------------------------------------------------

def masks_field(kind):
    """(wet, ao) per texel in Blender pixel order (row 0 = V 0), as module doc."""
    c = (np.arange(MASK_SIZE, dtype=np.float64) + 0.5) / MASK_SIZE * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    if kind == "straight":
        t = ys
        centres = np.array(sleeper_angles_or_x(kind))
        along = np.min(np.abs(xs[..., None] - centres[None, None, :]), axis=-1)
        on = np.abs(xs) <= hexgeo.APOTHEM
    else:
        dx, dy = xs - BEND_C[0], ys - BEND_C[1]
        r = np.hypot(dx, dy)
        th = np.degrees(np.arctan2(dy, dx)) % 360.0
        t = r - BEND_R
        centres = np.array(sleeper_angles_or_x(kind))
        along = np.min(np.abs(th[..., None] - centres[None, None, :]), axis=-1)
        along = np.radians(along) * r
        on = (th >= BEND_FROM) & (th <= BEND_FROM + BEND_TURN)
    at = np.abs(t)
    gap = np.clip(along - SLEEPER_HALF_W, 0.0, None)
    crib = 0.78 + 0.17 * np.clip(gap / 0.2, 0.0, 1.0)
    ao = np.ones_like(xs)
    ao = np.where(on & (at <= SLEEPER_HALF_L), crib, ao)
    ao = np.where(on & (at > SLEEPER_HALF_L) & (at <= SHOULDER_T), 0.95, ao)
    rail = on & (np.abs(at - RAIL_T) <= 0.06)
    sleeper = on & (at <= SLEEPER_HALF_L) & (gap <= 0.0)
    ao = np.where(rail | sleeper, 1.0, ao)
    wet = np.full_like(xs, 0.9)
    wet = np.where(sleeper, 0.6, wet)
    wet = np.where(rail, 0.15, wet)
    return wet.ravel().astype(np.float32), ao.ravel().astype(np.float32)


def lifted_uv(obj):
    """Planar UV0 of the 32 m square (`uv.planar_cap_uv`) plus UV_LIFT per metre of height on both
    axes, so vertical faces (rail webs, sleeper sides) keep a non-degenerate UV for MikkTSpace."""
    uv.planar_cap_uv(obj)
    mesh = obj.data
    layer = mesh.uv_layers["UVMap"]
    for loop in mesh.loops:
        z = mesh.vertices[loop.vertex_index].co.z
        a, b = layer.data[loop.index].uv
        layer.data[loop.index].uv = (a + UV_LIFT * z, b + UV_LIFT * z)


# --- the piece ----------------------------------------------------------------------------------

def build_piece(piece, kind, segments, generator):
    """kind "straight" (lanes 0, 3) or "bend" (lanes 0, 1). Three LODs from `segments` path
    segments: LOD0 full sleepers, LOD1 sleeper tops only, LOD2 no sleepers. Analytic masks, box
    collider, lane sockets, GLB and reports."""
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    out.mkdir(parents=True, exist_ok=True)
    scene.reset_scene(piece)
    root = scene.empty(piece)
    mats = {
        BALLAST: materials.pbr_material(BALLAST, (0.205, 0.2, 0.21, 1.0), 0.95),
        SLEEPER: materials.pbr_material(SLEEPER, (*SLEEPER_COLOR, 1.0), SLEEPER_ROUGH),
        STEEL: materials.pbr_material(STEEL, (*STEEL_COLOR, 1.0), STEEL_ROUGH),
        HEAD: materials.pbr_material(HEAD, (*HEAD_COLOR, 1.0), HEAD_ROUGH),
    }
    frames_fn, sleepers, lanes = ((straight_frames, straight_sleepers(), (0, 3)) if kind == "straight"
                                  else (bend_frames, bend_sleepers(), (0, 1)))
    lods = []
    for i, n in enumerate(segments):
        m = track(frames_fn(n), sleepers if i < 2 else None, top_only=i == 1)
        o = m.obj(f"{piece}_lod{i}", mats)
        lifted_uv(o)
        o.data.shade_smooth()
        o.data.set_sharp_from_angle(angle=math.radians(30.0))
        o.parent = root
        lods.append(o)
        assert [mt.name for mt in o.data.materials] == list(SLOTS), [mt.name for mt in o.data.materials]
    tris = [geom.tri_count(o) for o in lods]
    print(f"{piece}: tris per LOD {tris}")
    errs = [e for o in lods for e in border_section_errors(o, lanes) + interior_errors(o, lanes)]
    assert not errs, "\n".join(errs)
    budgets.check(tris, "center", piece)

    signature = bake.receiver_signature([lods[0]])
    texel = uv.measure_texel_density(lods[0], MASK_SIZE)
    wet, ao = masks_field(kind)
    bake.make_masks(MASK_SIZE, 1.0, wet, 0.0, None, str(out / f"{piece}.masks.png"))
    _, px = bake.load_png(str(out / f"{piece}.masks.png"))
    px[:, 3] = ao
    bake.write_png(MASK_SIZE, px, str(out / f"{piece}.masks.png"))

    col = collider.collider_from(lods[2], "box", f"{piece}_collider")
    col.parent = root
    for n, k in enumerate(lanes):
        c = hexgeo.lane_frame(k, 0.0, LANE_WIDTH)["center"]
        export.add_socket(f"socket-lane-{n}", (*c, 0.0), root, kind="rail", edge=k, offset=0.0,
                          width=LANE_WIDTH)
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    track_path = {"kind": kind, "lanes": list(lanes)}
    if kind == "bend":
        track_path.update(centre=list(BEND_C), radiusM=BEND_R, turnDeg=BEND_TURN,
                    arcLengthM=BEND_R * math.radians(BEND_TURN))
    bake.write_bake_report(
        str(out / "bake-report.json"), receiverSignature=signature, device="none", samples=0, marginPx=0,
        channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}}, texelDensityPxPerM=texel,
        reason="no Cycles bake: the masks are an analytic field of (along, across) on the planar UV0",
        masks={"snow": 1.0, "porosity": {"ballast": 0.9, "sleeper": 0.6, "rail": 0.15},
               "ao": {"crib": [0.78, 0.95], "shoulder": 0.95, "sleeper_rail": 1.0}, "night": 0.0},
        section={"gaugeM": GAUGE, "railCentreM": RAIL_T, "laneWidthM": LANE_WIDTH,
                 "ballastTopM": BALLAST_TOP, "shoulderM": SHOULDER_T, "toeM": [TOE_T, TOE_Z],
                 "sinkM": [SINK_T, SINK_Z], "railTopM": RAIL_TOP,
                 "sleeper": {"lengthM": 2 * SLEEPER_HALF_L, "widthM": 2 * SLEEPER_HALF_W,
                             "topM": SLEEPER_TOP, "pitchM": SLEEPER_PITCH, "count": len(sleepers)}},
        trackPath=track_path, lodTris=tris, skipped=False)
    print(json.dumps({"piece": piece, "tris": tris, "texel": texel}))
    return root, lods
