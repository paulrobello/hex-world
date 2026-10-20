"""boat_sail: a small gaff-rigged sailing fishing boat (PRD §7.2 boats; owner-approved refs
assets-source/refs/boat_sail ref-1..2, D4 2026-10-20: about 5 m long with a 6 m mast, one gaff
mainsail and one jib (BQ9), tan sails, a deep blue hull with a white band, a varnished interior with
thwarts, floorboards and two wicker fish baskets). Ocean and river mouths only (a 6 m mast never
clears the bridges' 0.95 m arch). Scripted route; the hull loft, the part contract and the bake are
in `_boat.py`.

Frame: the origin plane is the water surface at rest (the runtime stands the origin at WATER_Y =
-1.4 m). The bow faces +x, the pivot is mid-hull on the centreline. The keel bottom sits DRAFT below
the water; the rudder stops above it.

Parts: `boat_sail_main_lod<n>` (the gaff mainsail with its boom and gaff), its node on the mast axis
at the gooseneck with the hull's orientation, swung about local up (runtime +Y, the mast) up to
MAIN_SWING either side; `boat_sail_jib_lod<n>`, its node on the jib's tack at the bowsprit end with
local +Z (runtime +Y) along the forestay, swung about that axis up to JIB_SWING either side. Both are
authored on the centreline (sheeted amidships); the runtime swings them to leeward. The generator
asserts both clear the hull, the mast and the shrouds over those sweeps."""
import math
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

from mathutils import Vector  # noqa: E402

import _boat as B  # noqa: E402
from lib import export, geom, scene  # noqa: E402

P = "boat_sail"
SLOTS = {
    "hull": ((0.024, 0.07, 0.15), 0.45, (1.0, 0.1, 0.0)),
    "band": ((0.66, 0.65, 0.61), 0.45, (1.0, 0.1, 0.0)),
    "wood": ((0.45, 0.22, 0.07), 0.4, (1.0, 0.25, 0.0)),
    "sail": ((0.37, 0.12, 0.05), 0.85, (1.0, 0.5, 0.0)),
    "wicker": ((0.25, 0.15, 0.06), 0.9, (1.0, 0.6, 0.0)),
}
H = {"L": 4.7, "HB": 0.95, "UM": 0.42, "WT": 0.62, "PB": 1.6, "F": 0.58, "SB": 0.28, "SS": 0.10, "UF": 0.5,
     "T": 0.20, "KB": 0.18, "KS": 0.06, "UK": 0.5, "N": 2.2, "TH": 0.03, "FLOOR": 0.25,
     "KEEL_W": 0.04, "KEEL_D": 0.12, "STEM": 0.045, "STEM_HEAD": 0.10, "RAKE": 0.30}
DRAFT = H["T"] + H["KEEL_D"]
BAND = [(0.035, "wood"), (0.2, "band")]
LOD = [
    {"nu": 12, "top": BAND, "below": "hull", "lower": 4, "inner": 4, "bow_deck": 9, "keel": True,
     "frames": [0.14, 0.24, 0.34, 0.44, 0.54, 0.64, 0.74], "floor": [-0.4, -0.2, 0.0, 0.2, 0.4], "mast": 8,
     "spar": 6, "rig": True, "basket": 8},
    {"nu": 7, "top": BAND, "below": "hull", "lower": 2, "inner": 2, "bow_deck": 5, "keel": True,
     "frames": [], "floor": [-0.25, 0.25], "mast": 5, "spar": 4, "rig": False, "basket": 5},
    {"nu": 4, "top": [(0.2, "band")], "below": "hull", "lower": 1, "inner": 1, "bow_deck": 3, "keel": False,
     "frames": [], "floor": [], "mast": 3, "spar": 3, "rig": False, "basket": 0},
]
SEAT_Z = 0.38
MAST_X, MAST_TOP, MAST_R0, MAST_R1 = 0.9, 6.0, 0.07, 0.05
HOUNDS_Z = 5.0
BOOM_Z = 1.05  # gooseneck height
BOOM_LEN, GAFF_THROAT, GAFF_LEN, GAFF_DEG = 3.35, 3.15, 2.3, 45.0
SPRIT_LEN = 0.7
MAIN_SWING, JIB_SWING = 70.0, 45.0
BASKETS = [(0.15, 0.27), (0.15, -0.27)]
BASKET_R, BASKET_H = 0.2, 0.32
FLOOR_Z = 0.09


def stem_head():
    return Vector((H["L"] / 2 + H["RAKE"], 0.0, B.zs_of(H, 1.0) + H["STEM_HEAD"] * 0.6))


def sprit_end():
    return stem_head() + Vector((SPRIT_LEN, 0.0, 0.08))


def forestay():
    """Tack (bowsprit end) and the forestay's top on the mast."""
    return sprit_end(), Vector((MAST_X + MAST_R1, 0.0, HOUNDS_Z))


def shroud_feet():
    u = B.u_of(H, MAST_X - 0.05)
    return [Vector((MAST_X - 0.05, s * B.hb_of(H, u), B.zs_of(H, u))) for s in (1, -1)]


def body(i):
    lod = LOD[i]
    m = B.Mesh(P, SLOTS)
    B.hull(m, H, lod, "wood", "wood", "hull", "hull")
    if lod["frames"]:
        B.frames(m, H, lod["frames"], "wood", rows=3, z_stop=FLOOR_Z + 0.01)
    skip = ("-z",) if i == 2 else ()
    B.thwart(m, H, MAST_X, SEAT_Z, 0.24, 0.04, "wood", skip)
    B.thwart(m, H, -0.35, SEAT_Z, 0.22, 0.04, "wood", skip)
    if i < 2:
        B.thwart(m, H, -1.95, SEAT_Z + 0.02, 0.45, 0.04, "wood", skip)
    if lod["floor"]:
        B.floorboards(m, H, lod["floor"], FLOOR_Z, (0.16, 0.76), "wood", ends=i == 0)
    # Mast from the floor through the mast thwart, a truck on top.
    m.tube((MAST_X, 0.0, FLOOR_Z - 0.02), (MAST_X, 0.0, MAST_TOP), MAST_R0, lod["mast"], "wood", rb=MAST_R1,
           caps=(False, True))
    # Bowsprit from the stem head.
    m.tube(stem_head() - Vector((0.25, 0.0, 0.02)), sprit_end(), 0.045, lod["spar"], "wood", rb=0.035,
           caps=(False, True))
    # Rudder hung on the transom (a double-sided board) and a tiller over the stern sheets.
    xt = -H["L"] / 2
    zr0, zr1 = B.zs_of(H, 0.0) + 0.12, -(DRAFT - 0.04)
    m.board([(xt - 0.04, 0.0, zr0), (xt - 0.04, 0.0, zr1), (xt - 0.42, 0.0, zr1 + 0.08), (xt - 0.3, 0.0, zr0 - 0.25)],
            (0, 1, 0), "hull", 0.03)
    if i < 2:
        m.tube((xt - 0.06, 0.0, zr0 - 0.03), (xt + 1.0, 0.0, zr0 - 0.08), 0.025, lod["spar"], "wood", caps=(True, True))
    if lod["basket"]:
        for x, y in BASKETS:
            m.tube((x, y, FLOOR_Z), (x, y, FLOOR_Z + BASKET_H), BASKET_R * 0.85, lod["basket"], "wicker", rb=BASKET_R,
                   caps=(False, True))
    if lod["rig"]:
        tack, top = forestay()
        m.tube(tack, top, 0.012, 3, "band")
        for f in shroud_feet():
            m.tube(f, Vector((MAST_X, 0.0, HOUNDS_Z)), 0.012, 3, "band")
    return m.obj(f"{P}_lod{i}")


def main_sail(i):
    """Boom, gaff and the four-sided gaff main in the node frame (origin on the mast axis at the
    gooseneck)."""
    n = LOD[i]["spar"]
    m = B.Mesh(P, SLOTS)
    g = math.radians(GAFF_DEG)
    throat = Vector((-MAST_R0, 0.0, GAFF_THROAT))
    peak = throat + Vector((-math.cos(g), 0.0, math.sin(g))) * GAFF_LEN
    clew = Vector((-BOOM_LEN + 0.08, 0.0, 0.06))
    m.tube((-MAST_R0, 0.0, 0.0), (-BOOM_LEN, 0.0, 0.05), 0.04, n, "wood", rb=0.03, caps=(False, True))
    if i < 2:
        m.tube(throat, peak, 0.035, n, "wood", rb=0.028, caps=(False, True))
    sail = [Vector((-MAST_R0 - 0.02, 0.0, 0.06)), clew, peak + Vector((0.04, 0.0, -0.05)),
            throat + Vector((-0.02, 0.0, -0.05))]
    m.board(sail, (0, 1, 0), "sail", 0.01)
    o = m.obj(f"{P}_main_lod{i}", location=(MAST_X, 0.0, BOOM_Z))
    o["kind"] = "sail"
    return o


def jib(i):
    """The jib triangle in the node frame: origin on the tack, local +Z up the forestay."""
    m = B.Mesh(P, SLOTS)
    tack, top = forestay()
    axis = (top - tack).normalized()
    head = tack + (top - tack) * 0.9
    clew = Vector((MAST_X + 0.55, 0.0, BOOM_Z - 0.05))
    q = B.quat_z_to(axis)
    inv = q.inverted()
    pts = [inv @ (p - tack) for p in (Vector((0, 0, 0)) + tack + axis * 0.05, head, clew)]
    n = inv @ Vector((0.0, 1.0, 0.0))
    m.board(pts, n, "sail", 0.01)
    o = m.obj(f"{P}_jib_lod{i}", location=tack, rotation=q)
    o["kind"] = "sail"
    return o


def sweep_errors(o, swing, axis_local=(0.0, 0.0, 1.0), lift=0.28):
    """Sweep a sail part about its node's local Z within ±swing: every vertex over the hull's plan stays
    `lift` over the sheer, clears the mast by 0.1 m, and the main stays inside the shrouds' bearing.
    Returns (errors, lowest margin over the sheer, nearest approach to the mast axis)."""
    e, worst, near = [], 1e9, 1e9
    co = [Vector(v.co) for v in o.data.vertices]
    for deg in range(-int(swing), int(swing) + 1, 5):
        mat = o.matrix_basis @ B.rot_z(deg)
        for c in co:
            w = (mat @ c.to_4d()).to_3d()
            u = B.u_of(H, w.x)
            if 0.0 <= u <= 1.0 and abs(w.y) < B.hb_of(H, u) + 0.1:
                margin = w.z - B.zs_of(H, u)
                worst = min(worst, margin)
                if margin < lift:
                    e.append(f"{o.name}: {deg} deg: vertex {tuple(round(t, 3) for t in w)} {margin:.3f} over the sheer")
            r = math.hypot(w.x - MAST_X, w.y)
            if o.name.startswith(f"{P}_jib") and w.z < MAST_TOP:
                near = min(near, r)
                if r < MAST_R0 + 0.1:
                    e.append(f"{o.name}: {deg} deg: {r:.3f} m from the mast")
    return e[:5], worst, near


def shroud_bearing():
    """Angle between the boom at rest (aft, -x) and a shroud's foot about the mast axis."""
    f = shroud_feet()[0]
    return 180.0 - math.degrees(math.atan2(f.y, f.x - MAST_X))


def main():
    out = B.out_dir()
    scene.reset_scene(P)
    root = scene.empty(P)
    B.make_materials(P, SLOTS)
    bodies = [body(i) for i in range(3)]
    low_in = [B.interior_low(b, {"wood", "wicker"}) for b in bodies]
    print(f"{P}: lowest inboard vertex per LOD {low_in} (>= {B.INTERIOR_MIN_Z})")
    assert min(low_in) >= B.INTERIOR_MIN_Z, low_in
    parts = [[main_sail(i), jib(i)] for i in range(3)]
    errs, rep = [], {}
    for lod in parts:
        for o, swing in zip(lod, (MAIN_SWING, JIB_SWING)):
            e, w, near = sweep_errors(o, swing)
            errs += e
            rep[o.name] = {"minOverSheerM": round(w, 3), "nearMastM": round(near, 3) if near < 1e8 else None}
    bearing = shroud_bearing()
    print(f"{P}: sail sweeps {rep}, shroud bearing {bearing:.1f} deg (main swing {MAIN_SWING}), errors {errs}")
    assert not errs, errs
    assert MAIN_SWING + 5.0 <= bearing, (bearing, MAIN_SWING)
    export.add_socket("anchor-prop-0", (-1.95, 0.0, SEAT_Z + 0.02), root, 0.0, kind="helm")
    export.add_socket("anchor-prop-1", tuple(stem_head()), root, 0.0, kind="painter")
    export.add_socket("anchor-prop-2", (MAST_X, 0.0, MAST_TOP), root, 0.0, kind="perch")
    col = B.box_collider(bodies[2], f"{P}_collider")
    xs = [v.co.x for v in bodies[0].data.vertices]
    ys = [v.co.y for v in bodies[0].data.vertices]
    zs = [v.co.z for v in bodies[0].data.vertices]
    hull_xs = [x for x, z in zip(xs, zs) if z < 1.0]
    geometry = {"lengthOverallM": max(xs) - min(xs), "hullLengthM": H["L"] + H["RAKE"], "beamM": max(ys) - min(ys),
                "draftM": -min(zs), "mastTopM": MAST_TOP, "sheerMidshipM": B.zs_of(H, 0.5), "waterlineZ": 0.0,
                "bow": "+x", "hullXRange": [min(hull_xs), max(hull_xs)]}
    tack, top = forestay()
    parts_rep = {"main": {"nodes": [lod[0].name for lod in parts], "kind": "sail",
                          "pivotBlender": [MAST_X, 0.0, BOOM_Z], "axisLocal": [0.0, 0.0, 1.0],
                          "swingDeg": MAIN_SWING, "shroudBearingDeg": bearing},
                 "jib": {"nodes": [lod[1].name for lod in parts], "kind": "sail",
                         "pivotBlender": list(tack), "axisLocal": [0.0, 0.0, 1.0],
                         "axisBlender": list((top - tack).normalized()), "swingDeg": JIB_SWING},
                 "sweeps": rep}
    print(f"{P}: geometry {geometry}")
    tris = B.finish(P, SLOTS, bodies, parts, col, out, __file__,
                    report_extra={"parts": parts_rep, "geometry": geometry},
                    bake_extra={"geometry": geometry, "parts": parts_rep})
    print(f"{P}: lodTris {tris}; body {[geom.tri_count(b) for b in bodies]}")


main()
