"""The wide rail bend, rail_bend_120_center (P3, refs `assets-source/refs/rail`, owner A17; owner
2026-10-20: rail bends like the road set): single track from the edge-0 midpoint to the edge-2
midpoint, two edges apart.

Geometry. A rail piece meets each lane edge square-on at its midpoint with no edge stub (`_rail.py`,
"Edge contract"), so the arc's centre lies on both edge lines: their intersection, Blender
(APOTHEM, -24), 24 m from both midpoints (the river bend's 24 m arc). The arc runs from 90 to 150
degrees about it, so the track turns 60 degrees over 8 pi = 25.133 m, three times the radius of
rail_bend_60_center's 8 m arc about corner 0.

Reuse. Everything but the lane pair comes from `_rail.py` unchanged: the section, ballast and rail
sweep (`track`), sleepers, the planar UV0, the analytic masks and the border checks. `_rail` reads
the bend's centre, radius, start and turn from module globals at call time (`masks_field`,
`sleeper_angles_or_x`), so they are set here at import. Its `build_piece` hard-codes lanes 0 and 1
for a bend, and `bend_point` froze r = 8 as a default argument, so this file builds its own frames
and sleepers with r passed explicitly and composes the piece from `_rail`'s parts with lanes 0 and 2.
"""
import json
import math
import pathlib
import sys

from lib import bake, budgets, collider, export, geom, hexgeo, materials, scene, uv

import _rail

A = hexgeo.APOTHEM
LANES = (0, 2)
R = A * math.sqrt(3.0)  # 24 m: the edge-0 / edge-2 line intersection to either midpoint
CENTRE = (A, -R)
FROM, TURN = 90.0, 60.0
ARC = R * math.radians(TURN)
SLEEPERS = round(ARC / _rail.SLEEPER_PITCH)
TILE_STEP = 2.5  # degrees between the tile polyline's interior points (data/tiles/rail_bend_120.ron)

_rail.BEND_C, _rail.BEND_R, _rail.BEND_FROM, _rail.BEND_TURN = CENTRE, R, FROM, TURN


def point(th):
    return _rail.bend_point(th, R)


def _close(p, q, tol=1e-9):
    return abs(p[0] - q[0]) < tol and abs(p[1] - q[1]) < tol


for _k, _th in zip(LANES, (FROM, FROM + TURN)):
    # Each end sits on its edge midpoint with the lateral along that edge's border line.
    _mid = hexgeo.lane_frame(_k, 0.0, _rail.LANE_WIDTH)["center"]
    assert _close(point(_th), _mid, 1e-6), (_k, point(_th), _mid)
    _lat = hexgeo.rotate_xy(_rail.u(_th), -_k)
    assert _close((abs(_lat[0]), abs(_lat[1])), (0.0, 1.0), 1e-9), (_k, _lat)
assert len(_rail.bend_sleepers()) == SLEEPERS  # the masks' cribs use the same sleeper count


def frames(n):
    return [(point(th), _rail.u(th)) for th in (FROM + TURN * s / n for s in range(n + 1))]


def sleepers():
    out = []
    for j in range(SLEEPERS):
        th = FROM + TURN * (j + 0.5) / SLEEPERS
        out.append((point(th), _rail.u(th), _rail.u(th + 90.0)))
    return out


def tile_points():
    """Interior points of the tile's network polyline every TILE_STEP degrees, runtime (x, z):
    Blender (x, y) is runtime (x, -y)."""
    n = round(TURN / TILE_STEP)
    out = []
    for s in range(1, n):
        x, y = point(FROM + TILE_STEP * s)
        out.append((round(x, 3), round(-y, 3)))
    return out


def build_piece(piece, segments, generator):
    """Three LODs from `segments` arc segments (LOD0 full sleepers, LOD1 sleeper tops, LOD2 none),
    as `_rail.build_piece` builds a bend, on lanes 0 and 2."""
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    out.mkdir(parents=True, exist_ok=True)
    scene.reset_scene(piece)
    root = scene.empty(piece)
    mats = {
        _rail.BALLAST: materials.pbr_material(_rail.BALLAST, (0.205, 0.2, 0.21, 1.0), 0.95),
        _rail.SLEEPER: materials.pbr_material(_rail.SLEEPER, (*_rail.SLEEPER_COLOR, 1.0), _rail.SLEEPER_ROUGH),
        _rail.STEEL: materials.pbr_material(_rail.STEEL, (*_rail.STEEL_COLOR, 1.0), _rail.STEEL_ROUGH),
        _rail.HEAD: materials.pbr_material(_rail.HEAD, (*_rail.HEAD_COLOR, 1.0), _rail.HEAD_ROUGH),
    }
    sl = sleepers()
    lods = []
    for i, n in enumerate(segments):
        m = _rail.track(frames(n), sl if i < 2 else None, top_only=i == 1)
        o = m.obj(f"{piece}_lod{i}", mats)
        _rail.lifted_uv(o)
        o.data.shade_smooth()
        o.data.set_sharp_from_angle(angle=math.radians(30.0))
        o.parent = root
        lods.append(o)
        assert [mt.name for mt in o.data.materials] == list(_rail.SLOTS), [mt.name for mt in o.data.materials]
    tris = [geom.tri_count(o) for o in lods]
    print(f"{piece}: tris per LOD {tris}")
    errs = [e for o in lods for e in _rail.border_section_errors(o, LANES) + _rail.interior_errors(o, LANES)]
    assert not errs, "\n".join(errs)
    budgets.check(tris, "center", piece)

    signature = bake.receiver_signature([lods[0]])
    texel = uv.measure_texel_density(lods[0], _rail.MASK_SIZE)
    wet, ao = _rail.masks_field("bend")
    bake.make_masks(_rail.MASK_SIZE, 1.0, wet, 0.0, None, str(out / f"{piece}.masks.png"))
    _, px = bake.load_png(str(out / f"{piece}.masks.png"))
    px[:, 3] = ao
    bake.write_png(_rail.MASK_SIZE, px, str(out / f"{piece}.masks.png"))

    col = collider.collider_from(lods[2], "box", f"{piece}_collider")
    col.parent = root
    for n, k in enumerate(LANES):
        c = hexgeo.lane_frame(k, 0.0, _rail.LANE_WIDTH)["center"]
        export.add_socket(f"socket-lane-{n}", (*c, 0.0), root, kind="rail", edge=k, offset=0.0,
                          width=_rail.LANE_WIDTH)
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(
        str(out / "bake-report.json"), receiverSignature=signature, device="none", samples=0, marginPx=0,
        channels={"masks": {"size": _rail.MASK_SIZE, "colorspace": "linear"}}, texelDensityPxPerM=texel,
        reason="no Cycles bake: the masks are an analytic field of (along, across) on the planar UV0",
        masks={"snow": 1.0, "porosity": {"ballast": 0.9, "sleeper": 0.6, "rail": 0.15},
               "ao": {"crib": [0.78, 0.95], "shoulder": 0.95, "sleeper_rail": 1.0}, "night": 0.0},
        section={"gaugeM": _rail.GAUGE, "railCentreM": _rail.RAIL_T, "laneWidthM": _rail.LANE_WIDTH,
                 "ballastTopM": _rail.BALLAST_TOP, "shoulderM": _rail.SHOULDER_T,
                 "toeM": [_rail.TOE_T, _rail.TOE_Z], "sinkM": [_rail.SINK_T, _rail.SINK_Z],
                 "railTopM": _rail.RAIL_TOP,
                 "sleeper": {"lengthM": 2 * _rail.SLEEPER_HALF_L, "widthM": 2 * _rail.SLEEPER_HALF_W,
                             "topM": _rail.SLEEPER_TOP, "pitchM": _rail.SLEEPER_PITCH, "count": len(sl)}},
        trackPath={"kind": "bend", "lanes": list(LANES), "centre": list(CENTRE), "radiusM": R,
                   "fromDeg": FROM, "turnDeg": TURN, "arcLengthM": ARC, "segments": list(segments),
                   "tilePointStepDeg": TILE_STEP, "tilePoints": [list(p) for p in tile_points()]},
        lodTris=tris, skipped=False)
    print(json.dumps({"piece": piece, "tris": tris, "texel": texel, "tilePoints": tile_points()}))
    return root, lods
