"""A red field poppy for the single-stem wildflowers (`wildflower_single_e`), drawn by the family's
own pipeline: `_single.build`, `_single.atlas` and `_props`' painters, unchanged and driven through
their module globals, so the cards, LODs (14/6/2 tris), wind data, masks, maps, collider and export
are exactly the siblings'. Refs: assets-source/refs/meadow_props (the meadow's stylised scale and
palette; the reference shows no poppy, the species comes from the brief).

What this module adds:
- `"poppy"` in `_props.SPECIES`, so `_single.build` accepts it.
- `flower_head`, which wraps `_props.flower_head` (looked up by global name in `_single.side_tile`
  and `_single.top_tile`) and draws the poppy for that species only, every other species delegated
  unchanged: from above (the head card) four broad scarlet petals, crinkled and veined, overlapping
  in pairs, over a near-black basal blotch, a ring of dark stamens and a grey-green seed capsule
  with dark stigma rays; from the side (the two crossed cards) the same flower as an open cup seen
  a little from above: the bowl of the near petals under the elliptical mouth, which shows the
  inner petals, the blotch and the capsule.
- `side_tile`, which wraps `_single.side_tile` (looked up by global name in `_single.atlas`) and
  draws the poppy's own side tile (`poppy_side_tile`) with the family's canvas, blade painter,
  head placement, roughness, dilation and fill: lobed basal leaves, a hairy stem of constant width
  that bows in one seeded curve from the base to the bowl of the cup (the family's stem is a
  tapering blade whose seeded bend can leave its tip short of the head, which a single tall stem
  shows), and, on side tile 0 only, one nodding bud: a thin hairy stalk from the base that arches
  over into a shepherd's crook, with an egg-shaped green bud hanging from it, its seam showing a
  line of red. Everything stays MARGIN inside the tile's left, right and top edges.

Invariants relied on (asserted at import): `_single.atlas` calls `side_tile(species, seed, k)` by
global name; `_single.top_tile` calls `_props.flower_head` by module attribute with
(rgb, alpha, nrm, x, y, cx, cy, r, species, rng); `_single.build` checks `_props.SPECIES` at call
time; the side tiles are SIDE_W x SIDE_H with both axes in tile widths."""
from __future__ import annotations

import inspect
import json
import math
import pathlib
import random

import bpy
import numpy as np

import _props
import _single

SPECIES = "poppy"
# sRGB colours: scarlet petals, the deep red of their bases, the basal blotch, stamens, capsule.
SCARLET = np.array([0.88, 0.12, 0.06])
DEEP = np.array([0.55, 0.04, 0.03])
BLOTCH = np.array([0.07, 0.035, 0.045])
STAMEN = np.array([0.20, 0.13, 0.20])
CAPSULE = np.array([0.50, 0.55, 0.38])
STIGMA = np.array([0.22, 0.16, 0.22])
BUD = np.array([0.36, 0.49, 0.22])
HAIR = np.array([0.62, 0.70, 0.46])
BUD_TILE = 0  # the side tile that carries the bud (one bud per plant)
REACH = (0.15, 0.25)  # collider reach (m): FOOTPRINTS ground 0.25, the group's largest

_family_head = _props.flower_head
_family_side = _single.side_tile
assert "side_tile(species, seed, k)" in inspect.getsource(_single.atlas)
assert "_props.flower_head(rgb, alpha, nrm, x, y," in inspect.getsource(_single.top_tile)
assert _single.SIDE_W * 4 == _single.SIDE_H and _single.SIDE_ASPECT == 4
assert "assert species in _props.SPECIES" in inspect.getsource(_single.build)
assert SPECIES not in _props.SPECIES


def _put(rgb, alpha, nrm, inside, col, nx, ny):
    """Write an opaque region: colour (h, w, 3 or 3), tangent normal leaning (nx, ny)."""
    if not inside.any():
        return
    col = np.broadcast_to(col, rgb.shape)
    rgb[:] = np.where(inside[..., None], np.clip(col, 0, 1), rgb)
    alpha[:] = np.where(inside, 1.0, alpha)
    nx, ny = np.broadcast_to(nx, alpha.shape), np.broadcast_to(ny, alpha.shape)
    nz = np.sqrt(np.clip(1 - nx * nx - ny * ny, 0.05, 1))
    nrm[:] = np.where(inside[..., None], np.stack([nx, ny, nz], -1) * 0.5 + 0.5, nrm)


def _centre(rgb, alpha, nrm, x, y, cx, cy, r, sy, rng):
    """The poppy's middle, squashed vertically by `sy` (1 from above, less on a tilted face): the
    black basal blotch with four lobes, a ring of dark stamens on it, the capsule and its rays."""
    dx, dy = (x - cx) / r, (y - cy) / (r * sy)
    rad, ang = np.hypot(dx, dy), np.arctan2(dy, dx)
    ph = rng.uniform(0, 2 * math.pi)
    blotch = rad < 0.34 + 0.07 * np.cos(4 * (ang - ph))
    _put(rgb, alpha, nrm, blotch, BLOTCH, dx * 0.15, dy * 0.15)
    for _ in range(22):
        a, d = rng.uniform(0, 2 * math.pi), rng.uniform(0.15, 0.27)
        sx, sy2 = cx + d * r * math.cos(a), cy + d * r * sy * math.sin(a)
        dot = ((x - sx) / (0.035 * r)) ** 2 + ((y - sy2) / (0.035 * r * max(sy, 0.4))) ** 2 < 1.0
        _put(rgb, alpha, nrm, dot, STAMEN * rng.uniform(0.8, 1.25), 0.0, 0.0)
    cap = rad < 0.13
    rays = np.abs(np.sin(4 * (ang - ph))) < 0.22
    col = np.where((rays & (rad > 0.03))[..., None], STIGMA, CAPSULE * (1.0 - 0.25 * rad[..., None] / 0.13))
    _put(rgb, alpha, nrm, cap, col, dx * 0.5, dy * 0.5)


def top_head(rgb, alpha, nrm, x, y, cx, cy, r, rng):
    """From above: four broad petals, the outer pair first and the inner pair over them, each an
    ellipse pushed out from the centre with a crinkled rim, fine radial veins and a darker base."""
    ph = rng.uniform(0, 2 * math.pi)
    for k in (0, 2, 1, 3):
        a = ph + k * math.pi / 2 + rng.uniform(-0.12, 0.12)
        size = (1.0 if k % 2 == 0 else 0.9) * rng.uniform(0.95, 1.03)
        ex, ey = math.cos(a), math.sin(a)
        px, py = x - (cx + ex * 0.42 * r * size), y - (cy + ey * 0.42 * r * size)
        u, v = (px * ex + py * ey) / (0.6 * r * size), (-px * ey + py * ex) / (0.66 * r * size)
        d = np.hypot(u, v)
        theta = np.arctan2(v, u)
        crinkle = 1.0 + 0.045 * np.sin(9 * theta + rng.uniform(0, 6.3)) + 0.03 * np.sin(17 * theta)
        inside = d < crinkle
        rad = np.hypot(x - cx, y - cy) / r
        vein = 0.94 + 0.06 * np.cos(46 * np.arctan2(y - cy, x - cx) + 3.0 * rad)
        base = np.clip((rad - 0.25) / 0.55, 0, 1)[..., None]
        edge = 1.0 - 0.22 * np.clip((d - 0.82) / 0.18, 0, 1)[..., None]
        col = (DEEP + (SCARLET - DEEP) * base) * vein[..., None] * edge * rng.uniform(0.94, 1.05)
        _put(rgb, alpha, nrm, inside, col, u * ex * 0.3 - v * ey * 0.3, u * ey * 0.3 + v * ex * 0.3)
    _centre(rgb, alpha, nrm, x, y, cx, cy, r, 1.0, rng)


def cup_head(rgb, alpha, nrm, x, y, cx, cy, r, rng):
    """From the side, a little above: the near petals' bowl (a half ellipse, darker toward its base,
    with two overlap creases) under the mouth (an ellipse with a four-petal wavy rim) that shows the
    inner petals, the blotch and the capsule; the near rim catches a little light."""
    mx, my = cx, cy + 0.25 * r  # the mouth's centre
    ax, ay = 0.92 * r, 0.42 * r
    ph = rng.uniform(0, 2 * math.pi)
    dx, dy = (x - mx) / ax, (y - my) / ay
    ang = np.arctan2(dy, dx)
    rim = 1.0 + 0.08 * np.cos(4 * (ang - ph)) + 0.03 * np.sin(11 * ang)
    mouth = np.hypot(dx, dy) < rim
    bx, by = (x - mx) / ax, (y - my) / (0.95 * r)
    bowl = (bx * bx + by * by < 1.0) & (y < my)
    t = np.clip((y - (my - 0.95 * r)) / (0.95 * r), 0, 1)
    crease = 1.0 - 0.25 * np.exp(-(((np.abs(x - mx) - 0.33 * r) / (0.03 * r)) ** 2))
    col = (DEEP + (SCARLET - DEEP) * (0.35 + 0.65 * t)[..., None]) * crease[..., None]
    _put(rgb, alpha, nrm, bowl, col, bx * 0.55, by * 0.35 - 0.1)
    d = np.hypot(dx, dy) / rim
    inner = (SCARLET * 0.96 - (SCARLET - DEEP) * 0.4 * np.clip(1 - d, 0, 1)[..., None])
    lip = (d > 0.88) & (y < my)
    inner = np.where(lip[..., None], inner * 0.5 + np.minimum(SCARLET * 1.05, 1.0) * 0.5, inner)
    _put(rgb, alpha, nrm, mouth, inner, -dx * 0.3, -dy * 0.2 + 0.25)
    _centre(rgb, alpha, nrm, x, y, mx, my, r * 0.95, ay / ax, rng)


def flower_head(rgb, alpha, nrm, x, y, cx, cy, r, species, rng):
    """`_props.flower_head` with the poppy: the cup on a side tile (y runs past 1 tile width), the
    open flower on a top tile; any other species is the family's own head."""
    if species != SPECIES:
        return _family_head(rgb, alpha, nrm, x, y, cx, cy, r, species, rng)
    if float(y.max()) > 1.5:
        cup_head(rgb, alpha, nrm, x, y, cx, cy, r, rng)
    else:
        top_head(rgb, alpha, nrm, x, y, cx, cy, r, rng)


def _bezier(p, n):
    t = np.linspace(0.0, 1.0, n)[:, None]
    p = [np.array(q, np.float32) for q in p]
    return ((1 - t) ** 3) * p[0] + 3 * ((1 - t) ** 2) * t * p[1] + 3 * (1 - t) * t * t * p[2] + t ** 3 * p[3]


def _stalk(rgb, alpha, nrm, x, y, pts, w0, w1, colour):
    """A tapering stalk along a polyline (tile units), shaded across like a round stem, with a pale
    fringe of hair along both sides."""
    best = np.full(x.shape, np.inf, np.float32)
    along = np.zeros(x.shape, np.float32)
    side = np.zeros(x.shape, np.float32)
    n = len(pts) - 1
    for i in range(n):
        (x0, y0), (x1, y1) = pts[i], pts[i + 1]
        sx, sy = x1 - x0, y1 - y0
        ln2 = max(sx * sx + sy * sy, 1e-9)
        s = np.clip(((x - x0) * sx + (y - y0) * sy) / ln2, 0, 1)
        qx, qy = x - (x0 + s * sx), y - (y0 + s * sy)
        dist = np.hypot(qx, qy)
        closer = dist < best
        best = np.where(closer, dist, best)
        along = np.where(closer, (i + s) / n, along)
        ln = math.sqrt(ln2)
        side = np.where(closer, (qx * -sy + qy * sx) / ln, side)
    half = 0.5 * (w0 + (w1 - w0) * along)
    inside = best < half
    across = np.clip(side / np.maximum(half, 1e-6), -1, 1)
    grain = np.modf(np.abs(np.sin(x * 129.898 + y * 782.33)) * 43758.5453)[0]
    hair = (best >= half) & (best < half + 0.014) & (grain > 0.62)
    _put(rgb, alpha, nrm, hair, HAIR, 0.0, 0.0)
    col = colour[None, None, :] * (1.0 - 0.25 * across ** 2)[..., None]
    _put(rgb, alpha, nrm, inside, col, across * 0.45, 0.0)


def bud(rgb, alpha, nrm, x, y, rng):
    """One nodding bud on a side tile: a stalk from the base arching over at about 70% of the tile
    height into a crook, the bud hanging from it (an egg 0.15 x 0.22 tile widths, darker on its
    shaded side, a pale hairy rim and a red line where the sepals part). At 70% the bud hangs at
    about 0.5 m on the 0.8 m poppy, above most of the play grass (blades about 0.35 m)."""
    s = rng.choice((-1.0, 1.0))
    top = _single.SIDE_ASPECT
    bx = 0.5 + s * rng.uniform(0.24, 0.29)
    apex = top * rng.uniform(0.68, 0.73)
    rise = _bezier([(0.5, 0.05), (0.5 + s * 0.04, apex * 0.45), (bx - s * 0.06, apex + 0.05), (bx, apex)], 18)
    crook = _bezier([(bx, apex), (bx + s * 0.04, apex - 0.02), (bx + s * 0.05, apex - 0.08),
                     (bx + s * 0.035, apex - 0.14)], 6)
    pts = [tuple(p) for p in rise] + [tuple(p) for p in crook[1:]]
    _stalk(rgb, alpha, nrm, x, y, pts, 0.045, 0.028, _props.GRASS_DARK * 1.25)
    cx, cy = bx + s * 0.035, apex - 0.14 - 0.10
    lean = -s * 0.25  # the bud hangs a little back under its crook
    px, py = x - cx, y - cy
    u = px * math.cos(lean) + py * math.sin(lean)
    v = -px * math.sin(lean) + py * math.cos(lean)
    ex, ey = u / 0.075, v / (0.11 * np.where(v < 0, 0.9, 1.0))
    d = np.hypot(ex, ey)
    inside = d < 1.0
    shade = (0.8 + 0.25 * np.clip(-ex * s, -1, 1)) * (1.0 - 0.15 * d ** 2)
    col = BUD[None, None, :] * shade[..., None]
    col = np.where(((d > 0.82) & inside)[..., None], HAIR * 0.9, col)
    seam = inside & (np.abs(ex - 0.15 * s) < 0.09) & (ey < 0.2)
    col = np.where(seam[..., None], SCARLET * 0.85, col)
    _put(rgb, alpha, nrm, inside, col, ex * 0.5, ey * 0.4)
    reach = float(np.max(np.where(alpha > 0.5, np.abs(x - 0.5), 0)))
    assert reach <= 0.5 - _single.MARGIN, ("bud past the tile margin", reach)


def poppy_side_tile(seed, k):
    """The poppy from the side on the family's SIDE_W x SIDE_H tile, in the family's order and
    finish: three or four lobed basal leaves (`_props._blade` straps with short lobes off both
    sides), on tile BUD_TILE the nodding bud, then the hairy stem, which bends in a gentle S from
    the base to the bowl of the cup (a round stalk that keeps its width, unlike the family's
    tapering blade, so it always meets the head), then the cup (`flower_head`). Then the family's
    roughness, edge dilation and background fill."""
    rng = random.Random(f"{seed}:side:{k}")
    x, y, rgb, alpha, nrm = _single._canvas(_single.SIDE_W, _single.SIDE_H)
    g0, g1 = _props.GRASS_DARK, _props.GRASS_LIGHT
    margin = _single.MARGIN
    for i in range(rng.randint(3, 4)):
        side = -1 if i % 2 else 1
        bx = 0.5 + side * rng.uniform(0.0, 0.04)
        ln = rng.uniform(0.8, 1.3)
        ang = math.radians(rng.uniform(8, 20)) * side
        tx, ty = min(max(bx + math.sin(ang) * ln, margin), 1 - margin), 0.08 + math.cos(ang) * ln
        col = g0 + (g1 - g0) * rng.uniform(0.2, 0.5)
        _props._blade(rgb, alpha, nrm, x, y, bx, 0.06, tx, ty, rng.uniform(0.13, 0.16), col, rng)
        for j in range(3):
            f = 0.3 + 0.2 * j + rng.uniform(-0.04, 0.04)
            px, py = bx + (tx - bx) * f, 0.06 + (ty - 0.06) * f
            for lob in (-1, 1):
                lx = min(max(px + lob * rng.uniform(0.09, 0.13) * (1.1 - f), margin), 1 - margin)
                _props._blade(rgb, alpha, nrm, x, y, px, py, lx, py + rng.uniform(0.08, 0.13),
                              0.07 * (1.1 - f), col * rng.uniform(0.9, 1.05), rng)
    if k == BUD_TILE:
        bud(rgb, alpha, nrm, x, y, random.Random(f"{seed}:bud:{k}"))
    r = rng.uniform(*_single.HEAD_R)
    hx = 0.5 + rng.uniform(-0.04, 0.04)
    hy = _single.SIDE_ASPECT - margin - r - rng.uniform(0.0, 0.05)
    s = rng.choice((-1.0, 1.0))
    bend = rng.uniform(0.14, 0.2)
    stem = _bezier([(0.5, 0.04), (0.5 + s * bend, hy * 0.35), (hx + s * bend * 0.8, hy * 0.72),
                    (hx, hy - 0.45 * r)], 28)
    _stalk(rgb, alpha, nrm, x, y, [tuple(p) for p in stem], 0.065, 0.05, g0 * 1.2)
    flower_head(rgb, alpha, nrm, x, y, hx, hy, r, SPECIES, rng)
    rough = np.where(alpha > 0.5, 0.7, 0.8).astype(np.float32)
    rgb = _props.dilate(rgb, alpha)
    rgb[alpha <= 0.5] = np.where(rgb[alpha <= 0.5].sum(-1, keepdims=True) > 0, rgb[alpha <= 0.5], g0)
    return np.concatenate([rgb, alpha[..., None]], -1), nrm, rough


def side_tile(species, seed, k):
    """`_single.side_tile`, with the poppy's own tile (`poppy_side_tile`) for that species."""
    if species != SPECIES:
        return _family_side(species, seed, k)
    return poppy_side_tile(seed, k)


def _tile_stats(out: pathlib.Path, piece: str) -> dict:
    """Per tile, the linear mean colour of the opaque texels and their scarlet share (what a deep
    mip shows), from the written albedo."""
    w, px = bpy_png(out / f"{piece}.albedo.png")
    img = px.reshape(w, w, 4)
    stats = {}
    for name, rect in [(f"side_{k}", r) for k, r in enumerate(_single.SIDE_RECTS)] + \
                      [(f"top_{k}", r) for k, r in enumerate(_single.TOP_RECTS)]:
        t = img[_single.rect_slice(rect)]
        op = t[..., 3] > 0.5
        lin = t[..., :3][op]
        red = (lin[:, 0] > 0.25) & (lin[:, 0] > 3 * lin[:, 1])
        stats[name] = {"opaqueMeanLinear": [round(float(c), 4) for c in lin.mean(0)],
                       "scarletShare": round(float(red.mean()), 4)}
    return stats


def bpy_png(path: pathlib.Path):
    """The albedo as stored (sRGB values decoded to linear by Blender's sRGB tag)."""
    img = bpy.data.images.load(str(path))
    img.colorspace_settings.name = "sRGB"
    px = np.empty(img.size[0] * img.size[1] * 4, np.float32)
    img.pixels.foreach_get(px)
    w = img.size[0]
    bpy.data.images.remove(img)
    return w, px


def build(piece: str, out: pathlib.Path, generator: str, *, height: float) -> None:
    """`_single.build` for a poppy `height` m tall, then the reach check and the poppy's tile stats
    added to the bake report."""
    _props.SPECIES = _props.SPECIES + (SPECIES,)
    _props.flower_head = flower_head
    _single.side_tile = side_tile
    _single.build(piece, out, generator, species=SPECIES, height=height)
    col = bpy.data.objects[f"{piece}_collider"]
    reach = max(math.hypot(v.co.x, v.co.y) for v in col.data.vertices)
    assert REACH[0] <= reach <= REACH[1], ("collider reach outside the group footprint", reach)
    report = out / "bake-report.json"
    data = json.loads(report.read_text())
    data["poppyStats"] = {"tiles": _tile_stats(out, piece), "colliderReachM": round(reach, 4),
                          "budTile": BUD_TILE}
    report.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
    print(f"{piece}: collider reach {reach:.3f}, poppy stats {data['poppyStats']}")
