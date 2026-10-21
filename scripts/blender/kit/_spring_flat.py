"""The flattened meadow relief on river_spring_base (owner D6-Q1, 2026-10-20). Imported before
`_spring.build_spring_base`, it replaces `_spring.relief` and `_spring.shade_slope` (module globals,
looked up by `vertex_z` and the base builder) so the meadow around the stream and pool carves
`_terrain.hollow` hollows on a plane level with the border ring, as `_river_flat.py` does on the
river bases. `_spring.py` is unchanged, so river_spring_center (which seats its dressing on
`_spring.bed_z`, not on the relief) is not touched. The falloffs stay as `_spring.relief` had them."""
import _river
import _spring
import _terrain
from lib import hexgeo


def relief(x, y):
    fb = _spring.smoothstep(0.0, 2.0, hexgeo.dist_to_border(x, y))
    return -_spring.RELIEF * fb * _spring.bank_falloff(x, y) * _terrain.hollow(_river.undulation(x, y))


shade_slope = _river.cap_shade_slope(
    _spring.RELIEF, lambda x, y: _spring.bank_falloff(x, y) * _terrain.hollow(_river.undulation(x, y)),
    lambda x, y: _spring.seg_dist(x, y) <= _spring.BANK_TOP + 1e-6 or _spring.pool_r(x, y) <= _spring.POOL_TOP + 1e-6)

_spring.relief = relief
_spring.shade_slope = shade_slope


def _check():
    """Where the border falloff is 1 the shading slope is the relief's true slope; the meadow stays
    in [-RELIEF, 0]."""
    e = 1e-5
    for x, y in ((-8.0, 9.0), (4.0, -10.0), (-11.5, 0.0), (3.0, 7.9), (-9.0, -6.0)):
        assert hexgeo.dist_to_border(x, y) >= 2.0 and shade_slope(x, y) is not None, (x, y)
        fd = ((relief(x + e, y) - relief(x - e, y)) / (2 * e), (relief(x, y + e) - relief(x, y - e)) / (2 * e))
        assert all(abs(p - q) < 1e-6 for p, q in zip(shade_slope(x, y), fd)), (x, y, fd)
    zs = [relief(i * 0.5, j * 0.5) for i in range(-32, 33) for j in range(-32, 33)
          if hexgeo.dist_to_border(i * 0.5, j * 0.5) >= 0.0]
    assert -_spring.RELIEF <= min(zs) and max(zs) <= 0.0, (min(zs), max(zs))


_check()
