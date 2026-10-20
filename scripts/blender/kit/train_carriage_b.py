"""train_carriage_b: a four-wheeled brake third, the second carriage design, marshalled at the tail of
every train (engine, train_carriage, train_carriage_b), so a train shows two carriage designs. Refs:
assets-source/refs/train ref-1 and ref-2 (A17: "two carriages"), which draw two identical passenger
carriages; no approved reference draws a brake carriage, so this one keeps the drawn carriage's
body, roof, panelling, underframe and colours and changes only its tail end: a guard's compartment
with double luggage doors, a raised roof lookout over it (a "birdcage" lookout with windows on all
four sides) and an open end veranda with corner posts and railings under the roof's overhang.
Scripted route; the shared builder, frame, rail fit, curve check and the runtime recolour contract
are in _train.py.

Envelope: identical to train_carriage, so the consist spacing (hex_sim::rail::CONSIST, TRAIN_LENGTH),
the couplings and every clearance stay valid unchanged: 7.14 m over buffers (headstocks at |x| = 3.3,
buffer faces at 3.57), buffers at z 1.05 and |y| 0.85, axles at ±1.5 m with 0.45 m wheels, 2.96 m over
the footboards. Only the lookout rises above the drawn roof (crown 3.62 m), to 3.95 m above the rail
top, under the engine's 4.2 m chimney. +x faces the engine; the veranda and the lookout are at -x, the
tail of the train (the veranda leads when the tank engine propels). Painted (runtime colour): the body
sides, the door beading, the guard's end wall and the lookout. Dark charcoal: the roofs, the +x end,
the veranda floor and railings, the underframe, footboards, axleboxes and springs."""
import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _train  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[3]
LOD0_CEILING = 832  # train_carriage's LOD0 (brief: its tris are this piece's ceiling)

# train_carriage's envelope (scripts/blender/kit/train_carriage.py), restated and checked against its
# committed bake report below.
L = 3.3  # body end / headstock face
W = 1.35  # body half width
BODY = (1.2, 3.2)
EAVE_W, CROWN = 1.42, 3.62
BUF_Z, BUF_Y = 1.05, 0.85
AXLES = (1.5, -1.5)
WHEEL_R = 0.45
WIN_Z = (2.35, 2.95)

# The tail end.
VX = -2.7  # the guard's end wall; the veranda runs from the headstock (-L) to here
POST = 0.07  # veranda corner post section
RAIL_Z = ((2.15, 2.21), (1.66, 1.71))  # top and middle rails
GUARD = -1.2  # bulkhead between the guard's compartment (VX..GUARD) and the passengers
LUGGAGE = (-1.95, 0.55)  # luggage double doors: centre and half width
LOOK_X = (-2.6, -1.5)  # the roof lookout over the guard's compartment
LOOK_W, LOOK_Z, LOOK_CROWN = 1.0, (3.40, 3.80), 3.95
# Windows (x, width): two droplights in the luggage doors, then the passenger compartments.
WINDOWS = [(-2.22, 0.32), (-1.68, 0.32), (-0.55, 0.5), (0.25, 0.36), (1.0, 0.52), (1.95, 0.36), (2.75, 0.42)]
DOORS = (0.25, 1.95)  # passenger door centres, 0.7 m wide


def build(p, d):
    # Body sides (painted) from the guard's end wall to the +x end, the dark +x end, the guard's end wall
    # (painted: under the veranda roof and away from the kit sun, charcoal read as a hole) with its door
    # light, and the veranda floor.
    p.box((VX, -W, BODY[0]), (L, W, BODY[1]), "paint", skip=("+x", "-x", "+z"))
    p.quad([(L, -W, BODY[0]), (L, W, BODY[0]), (L, W, BODY[1]), (L, -W, BODY[1])], "dark", out=(1, 0, 0))
    p.quad([(VX, -W, BODY[0]), (VX, W, BODY[0]), (VX, W, BODY[1]), (VX, -W, BODY[1])], "paint", out=(-1, 0, 0))
    if d < 2:
        x = VX - 0.006
        p.quad([(x, -0.3, WIN_Z[0]), (x, 0.3, WIN_Z[0]), (x, 0.3, WIN_Z[1]), (x, -0.3, WIN_Z[1])], "glass",
               out=(-1, 0, 0))
    p.quad([(-L, -W, BODY[0]), (VX, -W, BODY[0]), (VX, W, BODY[0]), (-L, W, BODY[0])], "dark", out=(0, 0, 1))
    # The curved roof over the body and the veranda, and its underside (the veranda ceiling).
    p.arch(-L - 0.08, L + 0.08, EAVE_W, BODY[1], CROWN, (8, 4, 2)[d], "roof", end_surf="dark")
    p.quad([(-L - 0.08, -EAVE_W, BODY[1] - 0.001), (L + 0.08, -EAVE_W, BODY[1] - 0.001),
            (L + 0.08, EAVE_W, BODY[1] - 0.001), (-L - 0.08, EAVE_W, BODY[1] - 0.001)], "dark", out=(0, 0, -1))
    # The roof lookout: painted sides on the roof, its own curved roof, windows all round.
    p.box((LOOK_X[0], -LOOK_W, LOOK_Z[0]), (LOOK_X[1], LOOK_W, LOOK_Z[1]), "paint", skip=("-z",))
    if d < 2:
        p.arch(LOOK_X[0] - 0.05, LOOK_X[1] + 0.05, LOOK_W + 0.06, LOOK_Z[1], LOOK_CROWN, (4, 2)[d], "roof",
               end_surf="dark")
        lz = (LOOK_Z[0] + 0.07, LOOK_Z[1] - 0.08)
        for sy in (1, -1):
            y = sy * (LOOK_W + 0.006)
            p.quad([(LOOK_X[0] + 0.14, y, lz[0]), (LOOK_X[1] - 0.14, y, lz[0]), (LOOK_X[1] - 0.14, y, lz[1]),
                    (LOOK_X[0] + 0.14, y, lz[1])], "glass", out=(0, sy, 0))
        for sx, x in ((-1, LOOK_X[0] - 0.006), (1, LOOK_X[1] + 0.006)):
            p.quad([(x, -0.62, lz[0]), (x, 0.62, lz[0]), (x, 0.62, lz[1]), (x, -0.62, lz[1])], "glass",
                   out=(sx, 0, 0))
    # Side windows (glass, night-lit), 6 mm proud of the side.
    for sy in (1, -1):
        y = sy * (W + 0.006)
        if d < 2:
            for x, w in WINDOWS:
                p.quad([(x - w / 2, y, WIN_Z[0]), (x + w / 2, y, WIN_Z[0]), (x + w / 2, y, WIN_Z[1]),
                        (x - w / 2, y, WIN_Z[1])], "glass", out=(0, sy, 0))
        else:
            p.quad([(-2.4, y, WIN_Z[0]), (3.0, y, WIN_Z[0]), (3.0, y, WIN_Z[1]), (-2.4, y, WIN_Z[1])], "glass",
                   out=(0, sy, 0))
    # Door beading (passenger doors and the luggage double doors) and the waist moulding, LOD0 only.
    # Open-ended 2 cm strips: their end faces would cost tris for nothing seen.
    if d == 0:
        for sy in (1, -1):
            lo, hi = sorted((sy * W, sy * (W + 0.02)))
            inner = "-y" if sy > 0 else "+y"
            beads = [xc + e for xc in DOORS for e in (-0.35, 0.35)]
            beads += [LUGGAGE[0] - LUGGAGE[1], LUGGAGE[0], LUGGAGE[0] + LUGGAGE[1]]
            for xb in beads:
                p.box((xb - 0.025, lo, BODY[0] + 0.05), (xb + 0.025, hi, BODY[1] - 0.08), "paint",
                      skip=(inner, "+z", "-z"))
            p.box((VX + 0.05, lo, 2.2), (L - 0.05, hi, 2.25), "paint", skip=(inner, "+x", "-x"))
    # The veranda: corner posts up to the roof, the end rails between them and the side handrails.
    if d < 2:
        for sy in (1, -1):
            lo, hi = sorted((sy * (W - POST), sy * W))
            p.box((-L, lo, BODY[0]), (-L + POST, hi, BODY[1] - 0.001), "dark", skip=("+z", "-z"))
            if d == 0:
                lo, hi = sorted((sy * (W - 0.05), sy * W))
                p.box((-L + POST, lo, RAIL_Z[0][0]), (VX, hi, RAIL_Z[0][1]), "dark", skip=("+x", "-x"))
        for k, (z0, z1) in enumerate(RAIL_Z):
            if d == 0 or k == 0:
                p.box((-L, -W + POST, z0), (-L + 0.05, W - POST, z1), "dark", skip=("+y", "-y"))
    # Underframe (solebars and headstocks), buffers, footboards, axleboxes and springs: as train_carriage.
    p.box((-L, -1.3, 0.9), (L, 1.3, BODY[0]), "dark", skip=("+z",))
    for sx in (1, -1):
        for sy in (1, -1):
            a = sx * L
            if d == 0:
                p.cyl(0, a, a + sx * 0.22, (sy * BUF_Y, BUF_Z), 0.09, 6, "dark", caps=(False, False))
                p.cyl(0, a + sx * 0.22, a + sx * 0.27, (sy * BUF_Y, BUF_Z), 0.19, 8, "steel")
            elif d == 1:
                p.cyl(0, a, a + sx * 0.27, (sy * BUF_Y, BUF_Z), 0.16, 6, "steel", caps=(False, True))
    for sy in (1, -1):
        if d < 2:
            lo, hi = sorted((sy * 1.3, sy * 1.48))
            p.box((-L + 0.25, lo, 0.6), (L - 0.25, hi, 0.65), "dark")
            for xw in AXLES:
                lo, hi = sorted((sy * 0.93, sy * 1.07))
                p.box((xw - 0.2, lo, 0.28), (xw + 0.2, hi, 0.62), "dark", skip=("-z",))
                if d == 0:
                    p.box((xw - 0.62, lo, 0.62), (xw + 0.62, hi, 0.76), "dark", skip=("+z",))
        else:
            lo, hi = sorted((sy * _train.TREAD_IN, sy * _train.TREAD_OUT))
            p.box((-1.5 - WHEEL_R, lo, 0.02), (1.5 + WHEEL_R, hi, 2 * WHEEL_R), "wheel",
                  skip=("-y" if sy > 0 else "+y", "+z"))


def envelope_errors(lod0):
    """This piece against train_carriage's committed bake report: the same axles, wheels and length
    over buffers, no wider, and no taller than the engine."""
    ref = json.loads((REPO / "assets-source/kit/train_carriage/bake-report.json").read_text())["geometry"]
    eng = json.loads((REPO / "assets-source/kit/train_engine/bake-report.json").read_text())["geometry"]
    xs = [v.co.x for v in lod0.data.vertices]
    ys = [v.co.y for v in lod0.data.vertices]
    zs = [v.co.z for v in lod0.data.vertices]
    e = []
    if sorted(ref["axlesX"]) != sorted(AXLES) or abs(ref["wheelRadiusM"] - WHEEL_R) > 1e-6:
        e.append(f"axles {AXLES} r {WHEEL_R} differ from train_carriage {ref['axlesX']} r {ref['wheelRadiusM']}")
    for got, want, what in ((min(xs), ref["extent"]["x"][0], "rear buffer face"),
                            (max(xs), ref["extent"]["x"][1], "front buffer face")):
        if abs(got - want) > 1e-4:
            e.append(f"{what} at x {got:.4f}, train_carriage {want:.4f}")
    if max(abs(y) for y in ys) > ref["extent"]["y"][1] + 1e-4:
        e.append(f"half width {max(abs(y) for y in ys):.4f} over train_carriage's {ref['extent']['y'][1]:.4f}")
    if max(zs) > eng["heightAboveRailM"]:
        e.append(f"top {max(zs):.3f} m above the engine's {eng['heightAboveRailM']:.3f} m")
    return e


SPEC = {"piece": "train_carriage_b", "kind": "prop", "axles": AXLES, "wheel_r": WHEEL_R, "build": build}

_, LODS = _train.build_piece(SPEC, __file__)
TRIS = [len(o.data.polygons) for o in LODS]
ERRS = envelope_errors(LODS[0])
print(f"train_carriage_b: tris {TRIS} (LOD0 ceiling {LOD0_CEILING}), envelope errors {ERRS}")
assert TRIS[0] <= LOD0_CEILING, TRIS
assert not ERRS, ERRS
