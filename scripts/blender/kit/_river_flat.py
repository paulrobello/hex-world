"""The flattened meadow relief on the channel-cut river bases (river_straight_base, river_bend_base;
owner D6-Q1, 2026-10-20). Imported before `_river.build_base`, it replaces `_river.relief` and
`_river.shade_slope` (module globals, looked up when the cap is built) so the meadow part of the cap
carves `_terrain.hollow` hollows on a plane level with the border ring instead of sitting at the
swell's depth and rising to the ring over the border falloff. `_river.py` itself is unchanged, so
the pieces that import it for its profile and channel (mouths, bridges, river dressing) are not
touched. The falloffs stay as `_river.relief` had them: 0 at the border (2 m) and at the bank top
(2 m), value and slope, at most `_river.RELIEF` deep."""
import _river
import _terrain
from lib import hexgeo


def relief(x, y, d):
    fb = _river.smoothstep(0.0, 2.0, hexgeo.dist_to_border(x, y))
    fc = _river.smoothstep(_river.BANK_TOP, _river.BANK_TOP + 2.0, d)
    return -_river.RELIEF * fb * fc * _terrain.hollow(_river.undulation(x, y))


def shade_slope(layout):
    """`_river.cap_shade_slope` of `relief`: the bank-top falloff keeps its slope, the border
    falloff's rise is left out, the strip keeps its own normals (as `_river.shade_slope`)."""
    dist = _river.LAYOUTS[layout]["dist"]
    return _river.cap_shade_slope(
        _river.RELIEF,
        lambda x, y: (_river.smoothstep(_river.BANK_TOP, _river.BANK_TOP + 2.0, dist(x, y))
                      * _terrain.hollow(_river.undulation(x, y))),
        lambda x, y: dist(x, y) <= _river.BANK_TOP + 0.05)


_river.relief = relief
_river.shade_slope = shade_slope


def _check():
    """Where the border falloff is 1 the shading slope is the relief's true slope (straight: spots in
    a hollow, on the plane and on the bank-top falloff), and the meadow stays in [-RELIEF, 0]."""
    e = 1e-5
    spots = {"straight": ((0.0, 9.5), (3.0, -11.0), (0.0, 7.8), (-1.0, 4.0)), "bend": ((-4.0, 6.0), (0.0, 4.0))}
    for layout, pts in spots.items():
        dist, slope = _river.LAYOUTS[layout]["dist"], shade_slope(layout)
        z = lambda x, y: relief(x, y, dist(x, y))  # noqa: E731
        for x, y in pts:
            if hexgeo.dist_to_border(x, y) < 2.0 or dist(x, y) <= _river.BANK_TOP + 0.05:
                continue
            fd = ((z(x + e, y) - z(x - e, y)) / (2 * e), (z(x, y + e) - z(x, y - e)) / (2 * e))
            assert all(abs(p - q) < 1e-6 for p, q in zip(slope(x, y), fd)), (layout, x, y, fd)
    zs = [relief(i * 0.5, j * 0.5, abs(j * 0.5)) for i in range(-32, 33) for j in range(-32, 33)
          if hexgeo.dist_to_border(i * 0.5, j * 0.5) >= 0.0]
    assert -_river.RELIEF <= min(zs) and max(zs) <= 0.0, (min(zs), max(zs))


_check()
