"""train_engine: a small six-wheeled (0-6-0) branch-line steam tank engine (owner-approved refs
assets-source/refs/train ref-1 and ref-2, A17: "Steam tank engine as drawn; runtime colour on the
body, like the cars"). Scripted route; the shared builder, frame, rail fit, curve check and the
runtime recolour contract are in _train.py.

Proportions from ref-2's side elevation, scaled to the 1.6 m gauge: 8.4 m over the buffer beams
(8.94 m over buffers), 2.7 m wide, 4.2 m from rail top to the chimney cap, 1.24 m driving wheels on a
4.0 m wheelbase. Faces +x (smokebox end); the cab and bunker are at -x. Painted (runtime colour): the
running plate and buffer beams, side tanks, boiler cladding, dome, cab and bunker. Dark charcoal: the
smokebox, saddle, chimney, frames and buffer stocks."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _train  # noqa: E402

L = 4.2  # buffer beam face
W = 1.35  # running plate half width
PLATE = (1.10, 1.33)
BUF_Z, BUF_Y = 1.05, 0.85
AXLES = (2.0, 0.0, -2.0)
WHEEL_R = 0.62
BOILER_Z, BOILER_R = 2.37, 0.74


def buffers(p, d, x0):
    for sx in (1, -1):
        for sy in (1, -1):
            a = sx * x0
            if d == 0:
                p.cyl(0, a, a + sx * 0.22, (sy * BUF_Y, BUF_Z), 0.09, 6, "dark", caps=(False, False))
                p.cyl(0, a + sx * 0.22, a + sx * 0.27, (sy * BUF_Y, BUF_Z), 0.19, 8, "steel")
            elif d == 1:
                p.cyl(0, a, a + sx * 0.27, (sy * BUF_Y, BUF_Z), 0.16, 6, "steel", caps=(False, True))


def build(p, d):
    n = (16, 10, 6)[d]
    # Running plate (with valance) and the buffer beams.
    p.box((-L + 0.25, -W, PLATE[0]), (L - 0.25, W, PLATE[1]), "paint")
    for sx in (1, -1):
        lo, hi = sorted((sx * (L - 0.25), sx * L))
        p.box((lo, -W, 0.80), (hi, W, PLATE[1]), "paint")
    buffers(p, d, L)
    if d < 2:
        for sy in (1, -1):
            lo, hi = sorted((sy * 0.50, sy * 0.60))
            p.box((-L + 0.3, lo, 0.32), (L - 0.3, hi, PLATE[0]), "dark", skip=("+z",))
    # Smokebox, saddle and boiler.
    p.cyl(0, 2.15, 3.55, (0.0, BOILER_Z), 0.80, n, "dark", caps=(d < 2, True))
    p.box((2.2, -0.55, PLATE[1]), (3.5, 0.55, 1.75), "dark", skip=("-z",))
    p.cyl(0, -0.9, 2.15, (0.0, BOILER_Z), BOILER_R, n, "paint", caps=(False, False))
    if d == 0:
        p.box((3.55, -0.05, 2.15), (3.62, 0.05, 2.45), "steel", skip=("-x",))  # smokebox door handle
        p.cyl(2, 2.95, 3.3, (-0.45, 0.0), 0.09, 6, "brass", caps=(False, True))  # safety valves
    # Chimney and dome.
    if d < 2:
        p.cyl(2, 2.9, 3.95, (2.85, 0.0), 0.20, (10, 6)[d], "dark", caps=(False, False), r1=0.24)
        p.cyl(2, 3.95, 4.2, (2.85, 0.0), 0.31, (10, 6)[d], "dark", caps=(True, True))
        p.cyl(2, 2.85, 3.5, (0.75, 0.0), 0.38, (10, 6)[d], "paint", caps=(False, False))
        if d == 0:
            p.cyl(2, 3.5, 3.72, (0.75, 0.0), 0.38, 10, "paint", caps=(False, False), r1=0.31)
            p.cyl(2, 3.72, 3.88, (0.75, 0.0), 0.31, 10, "paint", caps=(False, True), r1=0.15)
        else:
            p.cyl(2, 3.5, 3.85, (0.75, 0.0), 0.38, 6, "paint", caps=(False, True), r1=0.2)
    else:
        p.cyl(2, 2.9, 4.2, (2.85, 0.0), 0.26, 4, "dark", caps=(False, True))
        p.box((0.45, -0.3, 2.85), (1.05, 0.3, 3.75), "paint", skip=("-z",))
    # Side tanks.
    for sy in (1, -1):
        lo, hi = sorted((sy * 0.62, sy * W))
        p.box((-1.6, lo, PLATE[1]), (1.1, hi, 2.72), "paint", skip=("-z",))
    # Cab, its roof and the bunker behind it.
    p.box((-3.55, -W, PLATE[1]), (-0.9, W, 3.72), "paint", skip=("-z", "+z"))
    p.arch(-3.66, -0.8, 1.42, 3.70, 4.02, (6, 3, 2)[d], "paint")
    p.quad([(-3.66, -1.42, 3.699), (-0.8, -1.42, 3.699), (-0.8, 1.42, 3.699), (-3.66, 1.42, 3.699)], "dark",
           out=(0, 0, -1))
    p.box((-L, -W, PLATE[1]), (-3.55, W, 2.72), "paint", skip=("-z", "+x"))
    if d < 2:
        p.box((-L + 0.05, -W + 0.05, 2.72), (-3.6, W - 0.05, 2.9), "coal", skip=("-z",))
    # Cab glass: the side window in each door and the spectacle plates front and back.
    for sy in (1, -1):
        y = sy * (W + 0.006)
        p.quad([(-2.55, y, 2.55), (-1.7, y, 2.55), (-1.7, y, 3.4), (-2.55, y, 3.4)], "glass", out=(0, sy, 0))
        if d < 2:
            p.cyl(0, -0.9, -0.88, (sy * 0.86, 3.15), 0.21, (8, 6)[d], "glass", caps=(False, True))
            p.cyl(0, -3.55, -3.57, (sy * 0.7, 3.15), 0.21, (8, 6)[d], "glass", caps=(False, True))
    # Lamps on the front beam and the bunker back.
    if d < 2:
        for sy in (1, -1):
            lo, hi = sorted((sy * 0.45, sy * 0.70))
            p.box((L - 0.22, lo, PLATE[1]), (L - 0.04, hi, PLATE[1] + 0.24), "lamp", skip=("-z",))
        p.box((-L - 0.02, -0.12, 2.3), (-L + 0.02, 0.12, 2.55), "lamp", skip=("+x",))
    # Coupling rods (static), or one dark slab per side for the wheels at LOD2.
    zr = WHEEL_R - 0.18
    for sy in (1, -1):
        if d < 2:
            lo, hi = sorted((sy * 0.93, sy * 0.975))
            p.box((-2.1, lo, zr - 0.05), (2.1, hi, zr + 0.05), "steel")
        else:
            lo, hi = sorted((sy * _train.TREAD_IN, sy * _train.TREAD_OUT))
            p.box((-2.0 - WHEEL_R, lo, 0.02), (2.0 + WHEEL_R, hi, 2 * WHEEL_R), "wheel",
                  skip=("-y" if sy > 0 else "+y", "+z"))


SPEC = {"piece": "train_engine", "kind": "prop", "axles": AXLES, "wheel_r": WHEEL_R, "build": build}

_train.build_piece(SPEC, __file__)
