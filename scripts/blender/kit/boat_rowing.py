"""boat_rowing: a small planked rowing boat (PRD §7.2 boats; owner-approved refs
assets-source/refs/boat_rowing ref-1..3, D4 2026-10-20: about 3.5 m by 1.4 m, white with one dark
blue band, a smooth planked hull (BQ4), a varnished timber interior with two thwarts, stern sheets,
slatted floorboards and two oars in bronze rowlocks). Scripted route; the hull loft, the part
contract and the bake are in `_boat.py`.

Frame: the origin plane is the water surface at rest (z = 0 here; the runtime stands the origin at
WATER_Y = -1.4 m). The bow faces +x, the pivot is mid-length on the centreline. The keel bottom sits
DRAFT below the water.

Parts: `boat_rowing_oar_port_lod<n>` (+y, runtime -z) and `boat_rowing_oar_stbd_lod<n>` (-y), each
node on its rowlock pin with the hull's orientation, the oar authored square to the boat with its
blade feathered flat just above the water. The runtime sweeps an oar about its node's local up axis
(runtime +Y) and dips it about local +X (runtime +X, fore and aft); the generator asserts the blade
and loom clear the hull over the stroke box and in the trailing pose (blades aft alongside the hull).

No rower figure: none of the refs draws one, and a seated figure would need its own arms to follow
the oars. `anchor-prop-0` (kind "seat") marks the rowing thwart's top, facing aft, for one later."""
import math
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

from mathutils import Vector  # noqa: E402

import _boat as B  # noqa: E402
from lib import export, geom, scene  # noqa: E402

P = "boat_rowing"
# (linear colour = manifest colour, roughness, (snow, porosity, night))
SLOTS = {
    "hull": ((0.70, 0.69, 0.66), 0.45, (1.0, 0.1, 0.0)),
    "band": ((0.016, 0.03, 0.10), 0.4, (1.0, 0.1, 0.0)),
    "wood": ((0.50, 0.25, 0.07), 0.35, (1.0, 0.25, 0.0)),
    "bronze": ((0.30, 0.17, 0.06), 0.35, (0.8, 0.1, 0.0)),
}
H = {"L": 3.3, "HB": 0.70, "UM": 0.45, "WT": 0.55, "PB": 1.7, "F": 0.42, "SB": 0.22, "SS": 0.06, "UF": 0.5,
     "T": 0.10, "KB": 0.10, "KS": 0.04, "UK": 0.5, "N": 2.3, "TH": 0.025, "FLOOR": 0.13,
     "KEEL_W": 0.03, "KEEL_D": 0.05, "STEM": 0.035, "STEM_HEAD": 0.07, "RAKE": 0.22}
DRAFT = H["T"] + H["KEEL_D"]
BAND = [(0.03, "hull"), (0.14, "band")]
LOD = [
    {"nu": 12, "top": BAND, "below": "hull", "lower": 4, "inner": 4, "bow_deck": 9, "keel": True,
     "frames": [0.12, 0.2, 0.28, 0.36, 0.44, 0.52, 0.6, 0.68, 0.76], "floor": [-0.32, -0.16, 0.0, 0.16, 0.32],
     "rowlock": 2, "oar": 6},
    {"nu": 7, "top": BAND, "below": "hull", "lower": 2, "inner": 2, "bow_deck": 5, "keel": True,
     "frames": [], "floor": [-0.2, 0.2], "rowlock": 1, "oar": 4},
    {"nu": 4, "top": [(0.14, "band")], "below": "hull", "lower": 1, "inner": 1, "bow_deck": 3, "keel": False,
     "frames": [], "floor": [], "rowlock": 0, "oar": 3},
]
SEAT_X, SEAT_Z, SEAT_W = 0.15, 0.25, 0.22
STERN_THWART_X = -0.75
STERN_SHEETS = (-1.62, -1.25, 0.27)
ROWLOCK_X = SEAT_X - 0.33
# Oar: s along the loom from the rowlock pin (negative inboard), total 2.37 m.
HANDLE, GRIP, BLADE0, SHOULDER, TIP = -0.62, -0.47, 1.10, 1.30, 1.75
LOOM_R, GRIP_R = 0.026, 0.018
BLADE_TIP_Z = 0.05  # authored blade tip height above the water
STROKE_YAW, STROKE_DIP, TRAIL_YAW = 40.0, 8.0, 80.0


def pivot(side):
    u = B.u_of(H, ROWLOCK_X)
    return Vector((ROWLOCK_X, side * (B.hb_of(H, u) - 0.015), B.zs_of(H, u) + 0.04))


PIVOT_Z = pivot(1).z
ALPHA = math.asin((PIVOT_Z - BLADE_TIP_Z) / TIP)


def body(i):
    lod = LOD[i]
    m = B.Mesh(P, SLOTS)
    B.hull(m, H, lod, "wood", "hull", "hull", "hull")
    if lod["frames"]:
        B.frames(m, H, lod["frames"], "wood")
    skip = ("-z",) if i == 2 else ()
    B.thwart(m, H, SEAT_X, SEAT_Z, SEAT_W, 0.035, "wood", skip)
    B.thwart(m, H, STERN_THWART_X, SEAT_Z, 0.2, 0.035, "wood", skip)
    if i < 2:
        x0, x1, z = STERN_SHEETS
        B.thwart(m, H, (x0 + x1) / 2, z, x1 - x0, 0.03, "wood", skip)
    if lod["floor"]:
        B.floorboards(m, H, lod["floor"], 0.075, (0.18, 0.76), "wood", ends=i == 0)
    for side in (1, -1):
        p = pivot(side)
        zs = B.zs_of(H, B.u_of(H, ROWLOCK_X))
        y0, y1 = sorted((side * (B.hb_of(H, B.u_of(H, ROWLOCK_X)) - 0.065), side * (B.hb_of(H, B.u_of(H, ROWLOCK_X)) + 0.005)))
        if lod["rowlock"] == 2:
            m.box((ROWLOCK_X - 0.07, y0, zs - 0.005), (ROWLOCK_X + 0.07, y1, zs + 0.02), "bronze", skip=("-z",))
            for dx in (-0.035, 0.035):
                m.tube((p.x + dx, p.y, zs + 0.02), (p.x + dx * 1.3, p.y, p.z + 0.04), 0.011, 4, "bronze",
                       caps=(False, True))
        elif lod["rowlock"] == 1:
            m.box((ROWLOCK_X - 0.05, p.y - 0.02, zs - 0.005), (ROWLOCK_X + 0.05, p.y + 0.02, p.z + 0.03), "bronze",
                  skip=("-z",))
    return m.obj(f"{P}_lod{i}")


def oar(i, side):
    """One oar in its node frame (origin on the rowlock pin), outboard toward side * y, the loom
    sloping down by ALPHA so the feathered blade's tip sits BLADE_TIP_Z above the water."""
    n = LOD[i]["oar"]
    m = B.Mesh(P, SLOTS)
    a = Vector((0.0, side * math.cos(ALPHA), -math.sin(ALPHA)))
    x = Vector((1.0, 0.0, 0.0))
    up = a.cross(x) if side > 0 else x.cross(a)
    if up.z < 0:
        up = -up
    if n == 6:
        m.tube(a * HANDLE, a * GRIP, GRIP_R, n, "wood", caps=(True, False), up=up)
        m.tube(a * GRIP, a * BLADE0, LOOM_R, n, "wood", up=up)
        blade = [a * BLADE0 + x * 0.024, a * SHOULDER + x * 0.068, a * TIP + x * 0.07,
                 a * TIP - x * 0.07, a * SHOULDER - x * 0.068, a * BLADE0 - x * 0.024]
    else:
        m.tube(a * HANDLE, a * BLADE0, LOOM_R, n, "wood", caps=(n == 4, False), up=up)
        blade = [a * BLADE0 + x * 0.04, a * TIP + x * 0.07, a * TIP - x * 0.07, a * BLADE0 - x * 0.04]
    m.board(blade, up, "wood", 0.012)
    name = f"{P}_oar_{'port' if side > 0 else 'stbd'}_lod{i}"
    o = m.obj(name, location=pivot(side))
    o["kind"] = "oar"
    return o


def oar_errors(o, side):
    """Sweep the oar over the stroke box (yaw ±STROKE_YAW about local Z, dip ±STROKE_DIP about local
    X) and the trailing pose (TRAIL_YAW aft, 5° up): every outboard vertex (s > 0.15) stays outside
    the hull's half breadth where it is below the sheer, and the inboard grip stays 0.1 m over the
    thwarts. Returns (errors, worst outboard margin, lowest grip height)."""
    e, worst, low_grip = [], 1e9, 1e9
    co = [Vector(v.co) for v in o.data.vertices]
    a = Vector((0.0, side * math.cos(ALPHA), -math.sin(ALPHA)))
    poses = [(yaw, dip) for yaw in range(-int(STROKE_YAW), int(STROKE_YAW) + 1, 5)
             for dip in (-STROKE_DIP, 0.0, STROKE_DIP)]
    poses.append((side * TRAIL_YAW, 5.0 * side))
    for yaw, dip in poses:
        mat = B.rot_z(yaw) @ B.rot_x(dip)
        for c in co:
            s = c.dot(a)
            w = o.location + (mat @ c.to_4d()).to_3d()
            if s > 0.15:
                u = B.u_of(H, w.x)
                if 0.0 <= u <= 1.0 and w.z < B.zs_of(H, u) + 0.03:
                    margin = abs(w.y) - B.hb_of(H, u)
                    worst = min(worst, margin)
                    if margin < 0.01:
                        e.append(f"{o.name}: yaw {yaw} dip {dip}: vertex {tuple(round(t, 3) for t in w)} inside the hull")
            elif s < -0.3:
                low_grip = min(low_grip, w.z)
                if w.z < SEAT_Z + 0.1:
                    e.append(f"{o.name}: yaw {yaw} dip {dip}: grip at z {w.z:.3f} over the thwart")
    return e[:5], worst, low_grip


def main():
    out = B.out_dir()
    scene.reset_scene(P)
    root = scene.empty(P)
    B.make_materials(P, SLOTS)
    bodies = [body(i) for i in range(3)]
    low_in = [B.interior_low(b, {"wood", "bronze"}) for b in bodies]
    print(f"{P}: lowest inboard vertex per LOD {low_in} (>= {B.INTERIOR_MIN_Z})")
    assert min(low_in) >= B.INTERIOR_MIN_Z, low_in
    parts = [[oar(i, 1), oar(i, -1)] for i in range(3)]
    errs, worst, low = [], 1e9, 1e9
    for lod in parts:
        for o, side in zip(lod, (1, -1)):
            e, w, g = oar_errors(o, side)
            errs += e
            worst, low = min(worst, w), min(low, g)
    print(f"{P}: oars: alpha {math.degrees(ALPHA):.1f} deg, pivot z {PIVOT_Z:.3f}, outboard margin {worst:.3f} m, "
          f"lowest grip z {low:.3f}, errors {errs}")
    assert not errs, errs
    export.add_socket("anchor-prop-0", (SEAT_X, 0.0, SEAT_Z), root, 180.0, kind="seat")
    export.add_socket("anchor-prop-1", (H["L"] / 2 + H["RAKE"] + H["STEM"], 0.0, B.zs_of(H, 1.0)), root, 0.0, kind="painter")
    col = B.box_collider(bodies[2], f"{P}_collider")
    xs = [v.co.x for v in bodies[0].data.vertices]
    ys = [v.co.y for v in bodies[0].data.vertices]
    zs = [v.co.z for v in bodies[0].data.vertices]
    geometry = {"lengthM": max(xs) - min(xs), "beamM": max(ys) - min(ys), "draftM": -min(zs),
                "sheerMidshipM": B.zs_of(H, 0.5), "heightM": max(zs), "waterlineZ": 0.0,
                "bow": "+x", "seat": [SEAT_X, 0.0, SEAT_Z]}
    parts_rep = {"oars": {"nodes": [o.name for lod in parts for o in lod], "kind": "oar",
                          "pivotBlender": {"port": list(pivot(1)), "stbd": list(pivot(-1))},
                          "loomDirLocal": {"port": [0.0, math.cos(ALPHA), -math.sin(ALPHA)],
                                           "stbd": [0.0, -math.cos(ALPHA), -math.sin(ALPHA)]},
                          "slopeDeg": math.degrees(ALPHA), "strokeYawDeg": STROKE_YAW, "strokeDipDeg": STROKE_DIP,
                          "trailYawDeg": TRAIL_YAW, "outboardMarginM": worst, "lowestGripZ": low,
                          "lengthM": TIP - HANDLE}}
    print(f"{P}: geometry {geometry}")
    tris = B.finish(P, SLOTS, bodies, parts, col, out, __file__,
                    report_extra={"parts": parts_rep, "geometry": geometry},
                    bake_extra={"geometry": geometry, "parts": parts_rep})
    print(f"{P}: lodTris {tris}; body {[geom.tri_count(b) for b in bodies]}")


main()
