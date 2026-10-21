"""Shared terrain relief of the grass bases: meadow_base, forest_base (and its forest_forest_edge_e<k>
overlays), the meadow part of the river and spring bases (`_river_flat.py`, `_spring_flat.py`), and
the pieces that stand on the meadow relief (fence_paddock, wheat_field, the hay bale variants).

Contract (unchanged): the border ring is at z = 0, the relief is in [-0.25, 0] and never above the
ring, so a road bed (+0.08 m), a pad top (+0.05 m) or a rail toe (+0.03 m) always covers it and
every DROP foot (below -0.25 m) stays buried.

Hollows on a plane (owner D6-Q1, 2026-10-20: flatten the border rise in geometry). The relief used to
be -0.25 * falloff * swell, swell in [0, 1] with a mean near 0.47, so the cap sat about 0.11 m below
the ring and rose to it over the last 2 m of every border: a ridge where two tiles meet, which cast
streaks in a low sun. With the ring fixed at 0 and the cap never above it, the only way the surface
stops rising toward the border is for most of it to sit at the ring's height. So the swell now
carves hollows only where it exceeds HOLLOW_FROM, (clamp((swell - 0.5) / 0.5))^2 (C1 where a hollow
starts), and everywhere else the cap is level with the ring. The mean height is about -0.014 m on
the meadow (was -0.10), and the falloff, now over FALLOFF = 3 m, only softens the banks of the
hollows that reach a border.

The cap shades with `*_shade_slope`: the relief's own slope under the falloff, leaving out the
falloff's rise to the ring (owner, 2026-10-14). Scalars only, except `undulation`/`mounds`, which also
take numpy (m = np) for the masks bakes."""
import math

from lib import hexgeo

FALLOFF = 3.0  # border falloff width (m): 0 at the ring (value and slope), 1 beyond
HOLLOW_FROM = 0.5  # swell value where a hollow starts
MEADOW_DEPTH = 0.25  # a full hollow (swell 1); the meadow's deepest point is -0.20 m
FOREST_SWELL, FOREST_MOUNDS = 0.15, 0.10  # forest_base: long swell hollows plus root-mound pits
FOREST_MOUND_BAND = (2.5, 5.0)  # the pits fade in over this distance from the border (m)
H = 1e-5  # central-difference step of the shading slopes


def smoothstep(a, b, x):
    t = min(max((x - a) / (b - a), 0.0), 1.0)
    return t * t * (3.0 - 2.0 * t)


def falloff(x, y):
    return smoothstep(0.0, FALLOFF, hexgeo.dist_to_border(x, y))


def undulation(x, y, m=math):
    """Deterministic meadow swell in [0, 1]: two crossed long waves plus a shorter ripple (the
    formula meadow_base has always used; the masks bakes evaluate it with m = numpy)."""
    s = (0.6 * m.sin(0.31 * x + 1.7) * m.cos(0.27 * y - 0.4)
         + 0.4 * m.sin(0.53 * x - 0.45 * y + 0.9))
    return 0.5 + 0.5 * s


def mounds(x, y, m=math):
    """Forest-floor root mounds and hollows in [0, 1], shorter than the meadow swell."""
    s = (0.5 * m.sin(0.9 * x + 0.3) * m.sin(0.8 * y + 1.1)
         + 0.5 * m.sin(1.3 * x - 1.1 * y + 2.0) * m.cos(0.7 * x + 1.4 * y))
    return 0.5 + 0.5 * s


def hollow(v):
    """0 up to HOLLOW_FROM, then rising as a square to 1 at v = 1 (zero slope where it starts)."""
    t = min(max((v - HOLLOW_FROM) / (1.0 - HOLLOW_FROM), 0.0), 1.0)
    return t * t


def meadow_depth(x, y):
    """The meadow's hollow depth before the border falloff (>= 0)."""
    return MEADOW_DEPTH * hollow(undulation(x, y))


def forest_depth(x, y):
    """forest_base's depth before the border falloff: swell hollows plus mound pits inside the rim."""
    d = hexgeo.dist_to_border(x, y)
    return (FOREST_SWELL * hollow(undulation(x, y))
            + FOREST_MOUNDS * smoothstep(*FOREST_MOUND_BAND, d) * hollow(mounds(x, y)))


def meadow_relief(x, y):
    return -falloff(x, y) * meadow_depth(x, y)


def forest_relief(x, y):
    return -falloff(x, y) * forest_depth(x, y)


def slope_under(depth, fall, x, y):
    """-fall * grad(depth) at (x, y): the relief -fall * depth's slope without the falloff's own."""
    f = fall(x, y)
    return (-f * (depth(x + H, y) - depth(x - H, y)) / (2 * H),
            -f * (depth(x, y + H) - depth(x, y - H)) / (2 * H))


def meadow_shade_slope(x, y):
    return slope_under(meadow_depth, falloff, x, y)


def forest_shade_slope(x, y):
    return slope_under(forest_depth, falloff, x, y)


def check(relief, shade_slope, spots):
    """Where the falloff is 1, the shading slope is the relief's true slope; the relief stays in
    [-0.25, 0] on a 0.5 m grid over the hex."""
    for x, y in spots:
        assert hexgeo.dist_to_border(x, y) >= FALLOFF, (x, y)
        fd = ((relief(x + H, y) - relief(x - H, y)) / (2 * H), (relief(x, y + H) - relief(x, y - H)) / (2 * H))
        assert all(abs(p - q) < 1e-6 for p, q in zip(shade_slope(x, y), fd)), (x, y, fd)
    n = int(hexgeo.HEX_R / 0.5)
    zs = [relief(i * 0.5, j * 0.5) for i in range(-n, n + 1) for j in range(-n, n + 1)
          if hexgeo.dist_to_border(i * 0.5, j * 0.5) >= 0.0]
    assert -0.25 <= min(zs) and max(zs) <= 0.0, (min(zs), max(zs))


SPOTS = ((0.0, 0.0), (3.1, -4.2), (-6.5, 2.7), (1.9, 7.4))
check(meadow_relief, meadow_shade_slope, SPOTS)
check(forest_relief, forest_shade_slope, SPOTS)
assert meadow_relief(hexgeo.APOTHEM, 0.0) == 0.0 and forest_relief(hexgeo.APOTHEM, 0.0) == 0.0
