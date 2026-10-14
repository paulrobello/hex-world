"""car_van: a mid-size panel van (owner-approved reference assets-source/refs/vehicles ref-2): tall
slab-sided body, short sloping bonnet, raked windscreen, glazed cab doors and blank cargo panels, a
sliding-door handle and twin rear-door windows. The front end is deliberately generic (owner): a plain
black bumper, a wide grille of two flat slats with no emblem and simple rectangular lamps, so it reads
as no real model. Scripted route; the shared builder, its frame and the runtime recolour contract
are in _car.py.

4.9 m long, 1.9 m wide, 1.95 m tall, 3.0 m wheelbase, 0.66 m wheels: real-metre scale. Faces +x."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _car  # noqa: E402

L = 2.45  # half length


def details(detail):
    """(centre, size, surface, skipped faces, deepest LOD that keeps it)."""
    lit = ("-x", "+y", "-y", "+z", "-z") if detail == 2 else ("-x",)
    rlit = ("+x", "+y", "-y", "+z", "-z") if detail == 2 else ("+x",)
    out = []
    for s in (1, -1):
        inner = "-y" if s > 0 else "+y"
        out += [
            ((L - 0.04, s * 0.66, 0.80), (0.10, 0.34, 0.14), "head", lit, 2),
            ((-L + 0.03, s * 0.84, 0.98), (0.08, 0.14, 0.34), "tail", rlit, 2),
            ((1.36, s * 1.01, 1.30), (0.10, 0.14, 0.20), "trim", (inner,), 0),
            ((0.78, s * 0.952, 0.98), (0.16, 0.02, 0.035), "trim", (inner,), 0),
            ((-0.25, s * 0.952, 0.98), (0.16, 0.02, 0.035), "trim", (inner,), 0),
            ((0.0, s * 0.955, 0.80), (4.10, 0.02, 0.06), "trim", (inner,), 0),
        ]
    out += [
        ((L - 0.03, 0.0, 0.50), (0.12, 1.86, 0.22), "trim", ("-x",), 1),
        ((L - 0.01, 0.0, 0.78), (0.05, 0.84, 0.18), "trim", ("-x",), 1),
        ((L + 0.005, 0.0, 0.75), (0.02, 0.80, 0.025), "chrome", ("-x",), 0),
        ((L + 0.005, 0.0, 0.81), (0.02, 0.80, 0.025), "chrome", ("-x",), 0),
        ((L + 0.03, 0.0, 0.50), (0.02, 0.46, 0.11), "plate", ("-x",), 1),
        ((-L + 0.03, 0.0, 0.50), (0.12, 1.86, 0.20), "trim", ("+x",), 1),
        ((-L - 0.035, 0.0, 0.50), (0.02, 0.46, 0.11), "plate", ("+x",), 1),
        ((-2.40, 0.0, 1.50), (0.05, 0.07, 0.80), "trim", ("+x",), 0),
    ]
    return out


SPEC = {
    "piece": "car_van",
    "wheel_r": 0.33, "tyre_w": 0.215, "track": 0.80, "axles": (1.55, -1.45),
    "sill": [(-L, 0.42), (-2.3, 0.36), (2.3, 0.36), (L, 0.42)],
    "top": [(-L, 1.0), (-2.38, 1.04), (1.5, 1.04), (2.1, 0.98), (L, 0.92)],
    "half_width": [(-L, 0.92), (-2.35, 0.95), (2.2, 0.95), (L, 0.88)],
    "lod1_x": (-L, -2.38, 1.5, 2.1, L),
    "lod2_x": (-1.8, 0.0, 1.5, 2.1),
    "greenhouse": {"z": (1.04, 1.95), "rear": (-2.40, -2.36), "front": (1.55, 0.80),
                   "half_width": (0.93, 0.86), "chamfer": 0.12, "pillar": "paint",
                   "cuts": (0.30,), "side_glass": [(0.30, 1.6)], "pillars": []},
    "details": details,
}

_car.build_piece(SPEC, __file__)
