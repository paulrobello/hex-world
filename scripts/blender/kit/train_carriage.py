"""train_carriage: a short four-wheeled branch-line passenger carriage (owner-approved refs
assets-source/refs/train ref-1 and ref-2, A17: "two carriages"; the consist is train_engine plus two
of these). Scripted route; the shared builder, frame, rail fit, curve check and the runtime recolour
contract are in _train.py.

Proportions from ref-2's side elevation, scaled to the 1.6 m gauge: a 6.6 m body (7.14 m over
buffers), 2.7 m wide (2.84 m over the roof eaves and footboards stay inside 3.0 m), 3.62 m from rail
top to the roof crown, 0.9 m wheels on a 3.0 m wheelbase. Symmetric fore and aft; +x is its front.
Painted (runtime colour): the body sides with their door beading. Dark charcoal: the curved roof, the
boarded ends, the underframe, footboards, axleboxes and springs."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _train  # noqa: E402

L = 3.3  # body end / headstock face
W = 1.35  # body half width
BODY = (1.2, 3.2)
EAVE_W, CROWN = 1.42, 3.62
BUF_Z, BUF_Y = 1.05, 0.85
AXLES = (1.5, -1.5)
WHEEL_R = 0.45
WINDOWS = [(-2.75, 0.42), (-1.95, 0.36), (-1.0, 0.52), (1.0, 0.52), (1.95, 0.36), (2.75, 0.42)]  # (x, width)
WIN_Z = (2.35, 2.95)
DOORS = (-1.95, 1.95)  # door centres, 0.7 m wide


def build(p, d):
    # Body sides (painted), dark boarded ends, the curved roof and its underside.
    p.box((-L, -W, BODY[0]), (L, W, BODY[1]), "paint", skip=("+x", "-x", "+z"))
    for sx in (1, -1):
        x = sx * L
        p.quad([(x, -W, BODY[0]), (x, W, BODY[0]), (x, W, BODY[1]), (x, -W, BODY[1])], "dark", out=(sx, 0, 0))
    p.arch(-L - 0.08, L + 0.08, EAVE_W, BODY[1], CROWN, (8, 4, 2)[d], "roof", end_surf="dark")
    p.quad([(-L - 0.08, -EAVE_W, BODY[1] - 0.001), (L + 0.08, -EAVE_W, BODY[1] - 0.001),
            (L + 0.08, EAVE_W, BODY[1] - 0.001), (-L - 0.08, EAVE_W, BODY[1] - 0.001)], "dark", out=(0, 0, -1))
    # Windows (glass, night-lit), 6 mm proud of the side.
    for sy in (1, -1):
        y = sy * (W + 0.006)
        if d < 2:
            for x, w in WINDOWS:
                p.quad([(x - w / 2, y, WIN_Z[0]), (x + w / 2, y, WIN_Z[0]), (x + w / 2, y, WIN_Z[1]),
                        (x - w / 2, y, WIN_Z[1])], "glass", out=(0, sy, 0))
        else:
            p.quad([(-3.0, y, WIN_Z[0]), (3.0, y, WIN_Z[0]), (3.0, y, WIN_Z[1]), (-3.0, y, WIN_Z[1])], "glass",
                   out=(0, sy, 0))
    # Door beading, the waist moulding and the door vents (LOD0 only).
    if d == 0:
        for sy in (1, -1):
            lo, hi = sorted((sy * W, sy * (W + 0.02)))
            inner = "-y" if sy > 0 else "+y"
            for xc in DOORS:
                for e in (-0.35, 0.35):
                    p.box((xc + e - 0.025, lo, BODY[0] + 0.05), (xc + e + 0.025, hi, BODY[1] - 0.08), "paint",
                          skip=(inner,))
                p.box((xc - 0.22, lo, 3.0), (xc + 0.22, hi, 3.12), "dark", skip=(inner,))
            p.box((-L + 0.05, lo, 2.2), (L - 0.05, hi, 2.25), "paint", skip=(inner,))
    # Underframe (solebars and headstocks), buffers, footboards, axleboxes and springs.
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
    # Roof vents.
    if d == 0:
        for x in (-2.2, 0.0, 2.2):
            p.cyl(2, CROWN - 0.06, CROWN + 0.1, (x, 0.0), 0.1, 6, "roof", caps=(False, True))


SPEC = {"piece": "train_carriage", "kind": "prop", "axles": AXLES, "wheel_r": WHEEL_R, "build": build}

_train.build_piece(SPEC, __file__)
