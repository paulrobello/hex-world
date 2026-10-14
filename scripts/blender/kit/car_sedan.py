"""car_sedan: a compact four-door saloon (owner-approved reference assets-source/refs/vehicles
ref-1, photoreal die-cast proportions): raked windscreen, short boot, body-colour pillars with a black
B pillar, rectangular headlamps either side of a slatted grille. Scripted route; the shared builder,
its frame and the runtime recolour contract are in _car.py.

4.24 m long, 1.72 m wide, 1.42 m tall, 2.55 m wheelbase, 0.62 m wheels: real-metre scale. Faces +x."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _car  # noqa: E402

L = 2.12  # half length


def details(detail):
    """(centre, size, surface, skipped faces, deepest LOD that keeps it)."""
    lit = ("-x", "+y", "-y", "+z", "-z") if detail == 2 else ("-x",)
    rlit = ("+x", "+y", "-y", "+z", "-z") if detail == 2 else ("+x",)
    out = []
    for s in (1, -1):
        out += [
            ((L - 0.05, s * 0.56, 0.65), (0.12, 0.34, 0.10), "head", lit, 2),
            ((-L + 0.04, s * 0.58, 0.80), (0.10, 0.32, 0.12), "tail", rlit, 2),
            ((0.64, s * 0.85, 1.00), (0.12, 0.10, 0.08), "paint", ("-y" if s > 0 else "+y",), 0),
            ((0.22, s * 0.848, 0.86), (0.14, 0.02, 0.03), "trim", ("-y" if s > 0 else "+y",), 0),
            ((-0.62, s * 0.848, 0.86), (0.14, 0.02, 0.03), "trim", ("-y" if s > 0 else "+y",), 0),
            ((-0.08, s * 0.865, 0.52), (1.62, 0.02, 0.05), "trim", ("-y" if s > 0 else "+y",), 0),
        ]
    out += [
        ((L - 0.03, 0.0, 0.62), (0.07, 0.62, 0.12), "trim", ("-x",), 1),
        ((L - 0.01, 0.0, 0.62), (0.03, 0.60, 0.015), "chrome", ("-x", "-z"), 0),
        ((L + 0.005, 0.0, 0.44), (0.03, 0.94, 0.08), "trim", ("-x",), 1),
        ((L + 0.012, 0.0, 0.53), (0.02, 0.44, 0.10), "plate", ("-x",), 1),
        ((-L - 0.005, 0.0, 0.62), (0.03, 0.44, 0.11), "plate", ("+x",), 1),
        ((-L + 0.01, 0.0, 0.46), (0.04, 1.30, 0.07), "trim", ("+x",), 1),
    ]
    return out


SPEC = {
    "piece": "car_sedan",
    "wheel_r": 0.31, "tyre_w": 0.20, "track": 0.72, "axles": (1.27, -1.28),
    "sill": [(-L, 0.40), (-1.95, 0.30), (1.95, 0.30), (L, 0.38)],
    "top": [(-L, 0.88), (-2.02, 0.94), (-1.7, 0.955), (-1.25, 0.94), (0.78, 0.94), (1.3, 0.87), (1.9, 0.80), (L, 0.73)],
    "half_width": [(-L, 0.76), (-1.95, 0.84), (-1.5, 0.86), (1.5, 0.86), (1.95, 0.83), (L, 0.74)],
    "lod1_x": (-L, -2.02, -1.25, 0.78, 1.9, L),
    "lod2_x": (-1.7, -0.2, 0.78, 1.7),
    "greenhouse": {"z": (0.94, 1.42), "rear": (-1.18, -0.64), "front": (0.80, 0.04),
                   "half_width": (0.78, 0.64), "chamfer": 0.10, "pillar": "paint",
                   "cuts": (-0.20,), "side_glass": [(-1.10, 0.75)], "pillars": [(-0.20, 0.05)]},
    "details": details,
}

_car.build_piece(SPEC, __file__)
