"""The river_fork dressing on the "fork" layout (`_river_fork.py`): `_river_dress.build_dressing`'s bank
stones at the waterline and reed clumps up the banks, along all three arms. Kept apart from
`_river_fork.py` so the base does not depend on `_river_dress.py` (provenance hashes every imported
helper).

`build_dressing` walks one centre line, joint to joint, through `_river_dress.centre_frame`; this
module replaces that function (a module global, looked up at call time) with one that sends the
fork's fraction s along the three arms in turn, each from its joint (t = 0) to the centre (t = 1),
and passes every other layout to the original. Placements past an arm's crease would land in the
next arm's channel, so `skip` drops any whose bed distance (`_river_fork.dist_fork`, the same
`LAYOUTS["fork"]["dist"]` that `build_dressing` seats them with) is under `MIN_DIST`."""
import _river
import _river_dress
import _river_fork
from lib import hexgeo

MIN_DIST = 3.4  # stones sit at 3.6-4.5 m from the centre line and reeds at 4.7-5.7 m
PER_SIDE = 21  # stones per side over the three arms: 7 per arm, about river_straight_center's spacing

_centre_frame = _river_dress.centre_frame


def centre_frame(layout, s):
    """Point and unit normal on the fork's arms at fraction s in [0, 1]: arm 0, then 2, then 4."""
    if layout != "fork":
        return _centre_frame(layout, s)
    i = min(int(3.0 * s), 2)
    t = 3.0 * s - i
    k = _river_fork.LANES[i]
    return hexgeo.rotate_xy((_river.JOINT_X * (1.0 - t), 0.0), k), hexgeo.rotate_xy((0.0, 1.0), k)


_river_dress.centre_frame = centre_frame


def skip(x, y):
    """Drops a placement that would stand in deeper water than the bank line it was drawn for."""
    return _river_fork.dist_fork(x, y) < MIN_DIST


def _check():
    for i, k in enumerate(_river_fork.LANES):
        p, n = centre_frame("fork", (i + 0.25) / 3.0)
        lx, ly = hexgeo.rotate_xy(p, -k)
        nx, ny = hexgeo.rotate_xy(n, -k)
        assert abs(ly) < 1e-9 and abs(lx - 0.75 * _river.JOINT_X) < 1e-9 and abs(nx) < 1e-9 and abs(ny - 1.0) < 1e-9
    assert centre_frame("straight", 0.25) == _centre_frame("straight", 0.25)


_check()
