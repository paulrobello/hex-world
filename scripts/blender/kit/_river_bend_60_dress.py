"""Dressing for the tight river bend (river_bend_60_center): `_river_dress.build_dressing` along the
`bend60` layout of `_river_bend_60.py`. Replaces `_river_dress.centre_frame` (a module global that
`build_dressing` looks up at call time) with a wrapper that serves `bend60` and leaves every other
layout to the original, and thins the short inner bank through `build_dressing`'s `skip`. Kept
apart from `_river_bend_60.py` so the base does not hash the dressing helper."""
import math

import _river_bend_60 as bend
import _river_dress

_centre_frame = _river_dress.centre_frame


def centre_frame(layout, s):
    """`_river_dress.centre_frame` plus `bend60`: point and unit normal (toward +offset) on the arc
    at fraction s, joint to joint."""
    if layout != "bend60":
        return _centre_frame(layout, s)
    a = bend.A0 + (bend.A1 - bend.A0) * s
    n = (math.cos(a), math.sin(a))
    return (bend.C60[0] + bend.R60 * n[0], bend.C60[1] + bend.R60 * n[1]), n


_river_dress.centre_frame = centre_frame


def inner_skip(x, y):
    """Thins the inner bank's dressing to its arc length (the inner waterline runs 7.2 m against
    23.9 m outside): `build_dressing` places 14 stones and 5 reed clumps per side at evenly spread
    fractions s; inside the arc keep the stones whose slot floor(14 s) is 1 mod 3 (5 of 14) and the
    reed clumps in slots 0, 2 and 4 (3 of 5). Reeds stand beyond 4.6 m from the centre line,
    stones inside it."""
    rho = math.hypot(x - bend.C60[0], y - bend.C60[1])
    if rho >= bend.R60:
        return False
    s = (math.atan2(y - bend.C60[1], x - bend.C60[0]) % (2.0 * math.pi) - bend.A0) / (bend.A1 - bend.A0)
    if bend.R60 - rho < 4.6:
        return int(math.floor(s * 14)) % 3 != 1
    return int(math.floor(s * 5)) % 2 != 0


def _check():
    """The wrapped frame lies on the centre line with a unit normal; the other layouts are untouched."""
    for frac in (0.0, 0.25, 0.5, 1.0):
        p, n = centre_frame("bend60", frac)
        assert bend.dist_bend60(*p) < 1e-9 and abs(math.hypot(*n) - 1.0) < 1e-12
    assert centre_frame("straight", 0.5) == _centre_frame("straight", 0.5)


_check()
