"""River fork geometry (the river_fork tile): a symmetric Y of river lanes on edges 0, 2 and 4 that
meet at the tile centre, as road_junction does for roads, so two rivers can join or one can split.

Bed (what the engine draws on this tile, `water::tiles` `WaterKind::River`, and pins in
`fork_bed_matches_the_kit_profile`), Blender frame, z = height:
  z = `_river.profile_z(d)`, d = the distance to the nearest arm, the segment from the centre to
  the lane point of edge k (k = 0, 2, 4).
Within 60 degrees of an arm's axis that arm is the nearest, so in its wedge the bed depends only on
the lateral offset o from the axis. Two wedges meet on the bisector, a crease where the bed folds
into a ridge between the arms (r = |o| / sin 60 for the profile at offset o). The water reaches
4.0 m from each axis and 4.62 m from the centre along the bisectors (the centre reads as a widened
pool); the bank tops meet the bisectors at 7.89 m, leaving about 6 m of meadow to the grass edges
1, 3 and 5. The joint row of each arm carries `_river.OFFS` exactly (the river_edge seam of
river_straight_base and river_bend_base).

Registered as `_river.LAYOUTS["fork"]`, so the approved builders take it unchanged:
`_river.build_base` (the channel-cut base, with `_river_flat`'s flattened meadow relief) and
`_river_dress.build_dressing` (bank stones and reeds, `_river_fork_dress.py`). `channel_cap` keys
each row by offset and joins neighbouring rows at equal offsets, so `fork_rows` gives each wedge
rows that run across the wedge from the crease V (row 0) to the joint (row n), each offset's line
divided evenly between its crease point and the joint: the row-to-row edges are then the lines of
constant offset, every vertex carries the exact profile, and an empty row between wedges keeps
`channel_cap` from joining rows of different wedges.
"""
import math

import _river
from lib import hexgeo

LANES = (0, 2, 4)
SIN60 = math.sin(math.pi / 3.0)
TAN60 = math.tan(math.pi / 3.0)
LANE_PTS = tuple(hexgeo.rotate_xy((hexgeo.APOTHEM, 0.0), k) for k in LANES)
WATER_BISECTOR = 4.0 / SIN60  # 4.62: the waterline on a bisector
BANK_BISECTOR = _river.BANK_TOP / SIN60  # 7.89: the bank tops meet on a bisector


def arm_dist(x, y, lane_pt):
    """Distance from (x, y) to the arm from the centre to `lane_pt`."""
    lx, ly = lane_pt
    t = min(max((x * lx + y * ly) / (lx * lx + ly * ly), 0.0), 1.0)
    return math.hypot(x - t * lx, y - t * ly)


def dist_fork(x, y):
    """Distance to the nearest arm: the bed is `_river.profile_z` of it."""
    return min(arm_dist(x, y, p) for p in LANE_PTS)


def bed_z(x, y):
    return _river.profile_z(dist_fork(x, y))


def crease_point(k, o):
    """The crease point of arm k's wedge at offset o: on the bisector at -60 k + 60 sign(o) degrees,
    |o| / sin 60 from the centre. Computed from the canonical bisector angle, so the two wedges that
    share a bisector produce identical coordinates (channel_cap merges them by position)."""
    if abs(o) < 1e-12:
        return (0.0, 0.0)
    deg = (-60.0 * k + math.copysign(60.0, o)) % 360.0
    a = math.radians(deg)
    r = abs(o) / SIN60
    return (r * math.cos(a), r * math.sin(a))


def fork_rows(n, inner=None):
    """Rows of the three wedges as lists of (x, y, offset), arm 0's first, an empty row between
    wedges. Row 0 of a wedge is its crease V, row n its joint row (`_river.joint_row`), and row j
    puts offset o at j / n of the way along the line of constant o between them. `inner` is ignored:
    every row carries OFFS, since the bed is exact along lines of constant offset."""
    rows = []
    for i, k in enumerate(LANES):
        if i:
            rows.append([])
        for j in range(n + 1):
            row = []
            for o in _river.OFFS:
                if j == 0:
                    x, y = crease_point(k, o)
                else:
                    x0 = abs(o) / TAN60
                    lx = _river.JOINT_X if j == n else x0 + (_river.JOINT_X - x0) * j / n
                    x, y = hexgeo.rotate_xy((lx, o), k)
                row.append((x, y, o))
            rows.append(row)
    return rows


_river.LAYOUTS["fork"] = {"lanes": LANES, "dist": dist_fork, "rows": fork_rows}


def _check():
    """The invariants this layout leans on: the joint rows are `_river.joint_row` (the seam), each
    crease point is equidistant from its two arms and carries the profile of its offset, every row
    point lies on the bed of its own offset, and the spot values the engine pin test repeats."""
    rows = fork_rows(3)
    wedges = [rows[0:4], rows[5:9], rows[10:14]]
    for k, w in zip(LANES, wedges):
        joint = [(x, y) for x, y, _o in w[-1]]
        want = _river.joint_row(k)
        assert all(math.dist(a, b) < 1e-9 for a, b in zip(joint, want)), k
        for row in w:
            for x, y, o in row:
                assert abs(dist_fork(x, y) - abs(o)) < 1e-6, (k, x, y, o, dist_fork(x, y))
    for k in LANES:
        for o in _river.OFFS:
            x, y = crease_point(k, o)
            d = sorted(arm_dist(x, y, p) for p in LANE_PTS)
            assert abs(d[0] - abs(o)) < 1e-9 and abs(d[1] - abs(o)) < 1e-9, (k, o, d)
    spots = (((0.0, 0.0), -2.95), ((hexgeo.APOTHEM, 4.0), -1.40), ((hexgeo.APOTHEM, 0.0), -2.95),
             ((WATER_BISECTOR * 0.5, WATER_BISECTOR * SIN60), -1.40),
             ((BANK_BISECTOR * 0.5, BANK_BISECTOR * SIN60), 0.0), ((-hexgeo.APOTHEM, 0.0), 0.0))
    for (x, y), want in spots:
        assert abs(bed_z(x, y) - want) < 0.006, ((x, y), bed_z(x, y), want)


_check()
