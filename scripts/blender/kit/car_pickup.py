"""car_pickup: a compact single-cab pickup (owner-approved reference assets-source/refs/vehicles
ref-3): short sloping bonnet, two-door cab with a small rear window, an open load bed with
body-colour walls, tailgate and a dark liner floor, a plain slatted grille. Scripted route; the shared
builder, its frame and the runtime recolour contract are in _car.py.

4.7 m long, 1.72 m wide, 1.78 m tall, 2.8 m wheelbase, 0.64 m wheels: real-metre scale. Faces +x."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _car  # noqa: E402

L = 2.35  # half length
BED_X = (-L, -0.04)


def details(detail):
    """(centre, size, surface, skipped faces, deepest LOD that keeps it)."""
    lit = ("-x", "+y", "-y", "+z", "-z") if detail == 2 else ("-x",)
    rlit = ("+x", "+y", "-y", "+z", "-z") if detail == 2 else ("+x",)
    bed_len = BED_X[1] - BED_X[0]
    out = []
    for s in (1, -1):
        inner = "-y" if s > 0 else "+y"
        out += [
            ((L - 0.03, s * 0.62, 0.78), (0.10, 0.30, 0.13), "head", lit, 2),
            ((-L + 0.02, s * 0.83, 1.02), (0.06, 0.08, 0.26), "tail", rlit, 2),
            ((1.06, s * 0.92, 1.18), (0.08, 0.10, 0.14), "trim", (inner,), 0),
            ((0.30, s * 0.862, 0.98), (0.15, 0.02, 0.035), "trim", (inner,), 0),
            # Bed side walls, standing on the bed floor (kept at every LOD: they are the bed's shape).
            (((BED_X[0] + BED_X[1]) / 2 + 0.02, s * 0.81, 1.04), (bed_len - 0.04, 0.07, 0.40), "paint", ("-z", "+x", "-x", inner) if detail == 2 else (), 2),
        ]
    out += [
        ((-L + 0.03, 0.0, 1.04), (0.06, 1.55, 0.40), "paint", ("-z", "+y", "-y", "+x") if detail == 2 else (), 2),
        ((L - 0.02, 0.0, 0.48), (0.12, 1.70, 0.20), "trim", ("-x",), 1),
        ((L - 0.01, 0.0, 0.75), (0.05, 0.80, 0.20), "trim", ("-x",), 1),
        ((L + 0.005, 0.0, 0.71), (0.02, 0.76, 0.025), "chrome", ("-x",), 0),
        ((L + 0.005, 0.0, 0.76), (0.02, 0.76, 0.025), "chrome", ("-x",), 0),
        ((L + 0.005, 0.0, 0.81), (0.02, 0.76, 0.025), "chrome", ("-x",), 0),
        ((L + 0.045, 0.0, 0.48), (0.02, 0.46, 0.11), "plate", ("-x",), 1),
        ((-L - 0.01, 0.0, 0.55), (0.10, 1.70, 0.14), "chrome", ("+x",), 1),
        ((-L - 0.065, 0.0, 0.70), (0.02, 0.46, 0.11), "plate", ("+x",), 1),
    ]
    return out


SPEC = {
    "piece": "car_pickup",
    "wheel_r": 0.32, "tyre_w": 0.205, "track": 0.73, "axles": (1.45, -1.35),
    "sill": [(-L, 0.42), (-2.2, 0.36), (2.2, 0.36), (L, 0.40)],
    "top": [(-L, 0.80), (-2.28, 0.84), (-0.06, 0.84), (-0.02, 1.02), (1.25, 1.02), (2.0, 0.96), (L, 0.88)],
    "half_width": [(-L, 0.84), (-2.25, 0.86), (2.1, 0.86), (L, 0.80)],
    "lod1_x": (-L, -2.28, -0.06, -0.02, 1.25, 2.0, L),
    "lod2_x": (-0.06, -0.02, 1.25),
    "bed": {"x": BED_X},
    "greenhouse": {"z": (1.02, 1.78), "rear": (0.0, 0.06), "front": (1.25, 0.55),
                   "half_width": (0.80, 0.74), "chamfer": 0.10, "pillar": "paint",
                   "side_glass": [(0.10, 1.3)], "pillars": []},
    "details": details,
}

_car.build_piece(SPEC, __file__)
