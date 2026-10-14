"""River mouth bed profile (docs/kit/water-contract.md §3a), shared by the mouth bases and centres.

Mirrors `crates/hex-render/src/water/tiles.rs` `Mouth::height`, which the engine draws every water
input from and the kit gate checks every LOD against. Pure math, Blender frame: Blender (x, y) is
runtime (x, -y). Two handednesses, each authored at rotation 0 and never mirrored by the engine:
- "r" (`river_mouth`): River on edge 3, the centre line runs from edge 3's midpoint along +x.
- "l" (`river_mouth_l`): River on edge 4, the mirror image of "r" across the coast's seaward axis
  (runtime 30 degrees, Blender -30 degrees), which maps edges 0<->1, 2<->5 and 3<->4 and keeps s.

Bed: y = min(coast_height(s), WATER + h(d, s)), with s the straight beach's seaward distance and d
the distance to the centre-line ray (from the River edge's lane point through the tile centre and
on out to sea). h is the river carve with an s-dependent half-width w(s) and centre depth D(s):
- w(s) = 4.33 + 1.08 * smoothstep(-8, 0, s)          (4.33 inland, 5.41 from the waterline on)
- D(s) = 1.55 - 1.19 * smoothstep(-8, 3, s)          (1.55 m inland, 0.36 m at the shelf joint)
- bed(d) = -(D - 0.05) * max(0, 1 - (d / w)^2) - 0.05
- bank(d) = smoothstep(0.7 w, w + 2.5, d)
- h = min(1.4, bed + (1.4 - bed) * bank)
Where s <= -8 (the grass reach and the whole River edge) h is the river channel's own profile, so the
River edge ring matches river_straight; beyond s = 3 the coast is deeper, so the ocean and beach
edges stay on the coast profile.
"""
import math

WATER = -1.4
LAND = 1.4
APOTHEM = 16.0 * math.sqrt(3.0) / 2.0
W_IN, W_SEA = 4.33, 5.41
D_IN, D_SHELF = 1.55, 0.36
W_RAMP = (-8.0, 0.0)
D_RAMP = (-8.0, 3.0)
BEACH_TOP_S, SHELF_S, DROP_END_S = -8.0, 3.0, 8.0
SHELF_SLOPE = 0.12
FLOOR = -4.0
COS30, SIN30 = math.cos(math.radians(30.0)), math.sin(math.radians(30.0))
RIVER_EDGE = {"r": 3, "l": 4}

# (handedness, Blender x, Blender y, expected bed y): engine spot values (tiles.rs tests).
SPOT = (
    ("r", -13.856406, 0.0, -2.95),
    ("r", -13.856406, 4.0, -1.3994),
    ("r", -6.0, 0.0, -2.7575),
    ("r", -3.0, 4.0, -1.6653),
    ("r", 0.0, 0.0, -1.9773),
    ("r", 3.4641, 0.0, -1.76),
    ("r", 8.0, 0.0, -3.7353),
    ("r", 0.0, 8.0, -0.35),
    ("l", -6.928203, 12.0, -2.95),
    ("l", 0.0, 0.0, -1.9773),
)

def smoothstep(a, b, x):
    t = min(max((x - a) / (b - a), 0.0), 1.0)
    return t * t * (3.0 - 2.0 * t)


def coast_height(s):
    """tiles.rs coast_height."""
    if s <= BEACH_TOP_S:
        rel = LAND
    elif s < 0.0:
        t = s / BEACH_TOP_S
        rel = LAND * t * (2.0 - t)
    else:
        rel = -SHELF_SLOPE * min(s, SHELF_S) + (FLOOR - WATER + SHELF_SLOPE * SHELF_S) * smoothstep(SHELF_S, DROP_END_S, s)
    return WATER + rel


def seaward(x, y):
    """s at Blender (x, y): runtime p . (cos 30, sin 30) with runtime z = -y."""
    return x * COS30 - y * SIN30


def axis(hand):
    """(lane point, unit direction) of the centre-line ray in Blender (x, y). Edge k's outward normal
    is at Blender angle -60 k degrees."""
    a = math.radians(-60.0 * RIVER_EDGE[hand])
    n = (math.cos(a), math.sin(a))
    return (n[0] * APOTHEM, n[1] * APOTHEM), (-n[0], -n[1])


def dist(hand, x, y):
    """Distance from Blender (x, y) to the centre-line ray."""
    (ax, ay), (ux, uy) = axis(hand)
    t = max((x - ax) * ux + (y - ay) * uy, 0.0)
    return math.hypot(x - (ax + ux * t), y - (ay + uy * t))


def width(s):
    return W_IN + (W_SEA - W_IN) * smoothstep(W_RAMP[0], W_RAMP[1], s)


def depth(s):
    return D_IN + (D_SHELF - D_IN) * smoothstep(D_RAMP[0], D_RAMP[1], s)


def channel(d, s):
    """Channel bed y at distance d from the centre line and seaward distance s."""
    w, dep = width(s), depth(s)
    bed = -(dep - 0.05) * max(0.0, 1.0 - (d / w) ** 2) - 0.05
    bank = smoothstep(0.7 * w, w + 2.5, d)
    return WATER + min(LAND, bed + (LAND - bed) * bank)


def height(hand, x, y):
    """Bed y (runtime, = Blender z) at Blender (x, y)."""
    s = seaward(x, y)
    return min(coast_height(s), channel(dist(hand, x, y), s))


def in_channel(hand, x, y, eps=1e-4):
    """True where the channel, not the coast, sets the bed (the `river_bed` slot)."""
    s = seaward(x, y)
    return channel(dist(hand, x, y), s) < coast_height(s) - eps


def check_spots():
    for hand, x, y, want in SPOT:
        got = height(hand, x, y)
        assert abs(got - want) < 2e-3, f"mouth {hand} at ({x}, {y}): {got:.4f} != {want}"
