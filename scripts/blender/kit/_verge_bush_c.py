"""Third road-verge bush (refs: assets-source/refs/road_straight, ref-1's "grass verges with tufts and small
bushes"): a flowering hawthorn-like bush, taller than it is wide, a deep fresh green mass dotted all over
with clusters of cream-white blossom, so at play distance it reads as a pale-flecked upright bush beside
bush_verge's dark round box and bush_verge_b's small yellow-green lump.

Pipeline: `_verge_bush.build` over `_shrub.build`, both unchanged and driven through their module
globals, so the cards, stems, wind data, maps, collider and export are the family's. What this family
adds is the blossom: `_shrub.atlas` looks `cluster_tile` up by global name at call time, so
`blossom_tile` wraps the family's tile and paints seeded corymbs of five-petal florets over the outer
tiles' leaves (alpha only grows, so the per-tile coverage cannot drop; everything stays inside
`BLOSSOM_REACH` of the tile centre, clear of the tile inset). The inner tile and the shade blob stay
leaf-only, so gaps still read as shade. The upright outline is the family's own ellipsoid with b > a,
made uneven by `spray_cards` (the family's `cards`, also looked up by global name): a few seeded
spray directions over the upper two thirds push the nearby cards outward, like arching shoots
(crown keys `sprays`, `spray`, `spray_width`). Card facings, vertex normals, wind, maps, collider and
export stay the family's. `build` then records per-tile blossom fractions and the linear mean colour of each tile's opaque
texels, with and without blossom, in the bake report (`blossomStats`): what a deep mip shows.

Seasonal blossom hook (PRD §9.4, docs/kit/hooks-contract.md, owner 2026-10-20: no blossom in autumn
and winter): the ORM map's A channel, which Bevy never reads and the family writes as 1, carries
1 - blossom coverage, texel for texel with the painted florets (`blossom_orm`, wrapping
`bake.write_png` for the ORM only). The manifest's `"blossom"` on the leaves slot holds the linear
`leaf` colour (the tiles' mean leaf colour, `leafMeanLinear`) and `flower` colour (the blossom
texels' mean, `blossomHook.flowerLinear`): out of season the hook shifts each texel by coverage x
(leaf - flower), which also clears the blossom's share of texels filtering blends with leaf.
Albedo, normal, masks, ORM RGB and the GLB are unchanged by the hook.

Invariants this module relies on (asserted below, so a change to `_shrub` fails here, not in play):
`_shrub.atlas` calls `cluster_tile` by global name with signature (px, seed, k, inner),
`_shrub.build` calls `cards` by global name with signature (seed, crown) and writes its maps
through `bake.write_png`, the 1024
atlas of four 480 px tiles with tile 3 as the inner tile, and `_verge_bush.build` takes every value
as a keyword with no defaults (module globals do not reach default arguments)."""
from __future__ import annotations

import inspect
import json
import math
import pathlib
import random

import numpy as np
from mathutils import Vector

import _shrub
import _verge_bush
from lib import bake

assert "cluster_tile" in _shrub.atlas.__code__.co_names, "_shrub.atlas no longer looks cluster_tile up by name"
assert "cards" in _shrub.build.__code__.co_names, "_shrub.build no longer looks cards up by name"
assert {"bake", "write_png"} <= set(_shrub.build.__code__.co_names), "_shrub.build no longer writes through bake.write_png"
assert _shrub.bake is bake, "_shrub and this module must share lib.bake"
assert list(inspect.signature(_shrub.cluster_tile).parameters) == ["px", "seed", "k", "inner"], \
    inspect.signature(_shrub.cluster_tile)
assert list(inspect.signature(_shrub.cards).parameters) == ["seed", "crown"], inspect.signature(_shrub.cards)
assert (_shrub.ATLAS, _shrub.TILE, _shrub.LEAF_TILES, _shrub.INNER_TILE) == (1024, 480, 4, 3)
_VB_KW = {"palette", "crown", "stem", "lods", "height", "reach_range", "core_r", "core_h"}
_vb_params = inspect.signature(_verge_bush.build).parameters
assert {n for n, p in _vb_params.items() if p.kind is p.KEYWORD_ONLY} == _VB_KW, list(_vb_params)
assert all(p.default is p.empty for p in _vb_params.values()), "a _verge_bush.build default would be bypassed"

# sRGB blossom: hawthorn's cream-white petals, a warm shaded underside, a pale green-yellow eye
# ringed with dark pink anthers.
PETAL = np.array((0xf4, 0xf0, 0xe2), np.float32) / 255
PETAL_SHADE = np.array((0xc9, 0xc2, 0xad), np.float32) / 255
EYE = np.array((0xc8, 0xc8, 0x7a), np.float32) / 255
ANTHER = np.array((0x8a, 0x2e, 0x4a), np.float32) / 255
BLOSSOM_REACH = 0.36  # tile units from the centre: every floret stays inside the leaf disc
CLUSTER_R = (0.085, 0.12)  # corymb radius, tile units (about 2.5-3.5 cm on a 0.28 m card)
FLORET_R = (0.024, 0.031)  # floret radius, tile units (about 0.7-0.9 cm)

SPRAY_KEYS = ("sprays", "spray", "spray_width")
SPRAY_ZF = (-0.15, 0.8)  # elevation range (zf) of the spray directions: the upper two thirds

_family_tile = _shrub.cluster_tile
_family_cards = _shrub.cards
_family_write_png = bake.write_png
CLUSTERS: dict[int, int] = {}  # set by `build`: corymbs per outer tile
STATS: dict[str, dict] = {}
PAINTED: dict[int, np.ndarray] = {}  # blossom texels per outer tile, set by `blossom_tile`
FLOWER: dict[int, tuple[np.ndarray, int]] = {}  # (linear RGB sum, count) of those texels


def spray_cards(seed: str, crown: dict) -> list[dict]:
    """The family's cards, pushed out along a few seeded spray directions (arching hawthorn shoots),
    so the upright ellipsoid gets an uneven, bumpy outline. A card moves by the sum of Gaussian bumps
    over its angle to each spray direction (`spray` amplitude, `spray_width` radians); the base,
    below every spray, stays on the ellipsoid."""
    cs = _family_cards(seed, crown)
    assert all({"p", "kind"} <= set(c) for c in cs), "card dict keys changed"
    rng = random.Random(f"{seed}:sprays")
    dirs = []
    for i in range(crown["sprays"]):
        az = 2 * math.pi * (i + rng.uniform(-0.3, 0.3)) / crown["sprays"]
        zf = rng.uniform(*SPRAY_ZF)
        rr = math.sqrt(1 - zf * zf)
        dirs.append((np.array((rr * math.cos(az), rr * math.sin(az), zf)), rng.uniform(0.7, 1.0)))
    cx, cz, a, b = crown["cx"], crown["cz"], crown["a"], crown["b"]
    for c in cs:
        d = c["p"] - Vector((cx, 0.0, cz))
        u = np.array((d.x / a, d.y / a, d.z / b))
        u /= max(np.linalg.norm(u), 1e-6)
        w = sum(amp * math.exp(-math.acos(float(np.clip(u @ s, -1, 1))) ** 2 / (2 * crown["spray_width"] ** 2))
                for s, amp in dirs)
        f = 1.0 + crown["spray"] * min(w, 1.0)
        c["p"] = Vector((cx + d.x * f, d.y * f, cz + d.z * (1.0 + 0.6 * (f - 1.0))))
    return cs


def _lin(c: np.ndarray) -> np.ndarray:
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def _opaque_mean(rgba: np.ndarray) -> list[float]:
    on = rgba[..., 3] > 0.5
    return [round(float(v), 4) for v in _lin(rgba[..., :3][on]).mean(axis=0)]


def _floret(rgba, nrm, rough, ao, painted, x, y, fx, fy, r, rng, light):
    """One five-petal floret at (fx, fy), radius r (tile units), drawn into a bounded window."""
    px = rgba.shape[0]
    x0, x1 = max(int((fx - r) * px), 0), min(int((fx + r) * px) + 2, px)
    y0, y1 = max(int((fy - r) * px), 0), min(int((fy + r) * px) + 2, px)
    sub = (slice(y0, y1), slice(x0, x1))
    dx, dy = x[sub] - fx, y[sub] - fy
    rad = np.hypot(dx, dy) / r
    ang = np.arctan2(dy, dx) - rng.uniform(0, 2 * math.pi)
    lobe = np.abs(np.cos(ang * 2.5)) ** 0.9
    inside = rad < 0.42 + 0.58 * lobe
    if not inside.any():
        return
    edge = np.clip(rad / (0.42 + 0.58 * lobe), 0, 1)
    col = PETAL_SHADE + (PETAL - PETAL_SHADE) * (light * (1.0 - 0.35 * edge ** 2))[..., None]
    eye = rad < 0.22
    col = np.where(eye[..., None], EYE * (0.8 + 0.2 * light), col)
    ring = (rad > 0.2) & (rad < 0.34) & (np.abs(np.cos(ang * 5.0 + 1.3)) > 0.82)
    col = np.where(ring[..., None], ANTHER, col)
    rgba[sub][..., :3] = np.where(inside[..., None], np.clip(col, 0, 1), rgba[sub][..., :3])
    rgba[sub][..., 3] = np.where(inside, 1.0, rgba[sub][..., 3])
    nx, ny = dx / r * 0.45, dy / r * 0.45
    nz = np.sqrt(np.clip(1 - nx * nx - ny * ny, 0.05, 1))
    nrm[sub] = np.where(inside[..., None], np.stack([nx, ny, nz], -1) * 0.5 + 0.5, nrm[sub])
    rough[sub] = np.where(inside, 0.72, rough[sub])
    ao[sub] = np.where(inside, np.maximum(ao[sub], 0.78 + 0.2 * light), ao[sub])
    painted[sub] |= inside


def blossom_tile(px: int, seed: str, k: int, inner: bool) -> tuple[np.ndarray, ...]:
    """The family's cluster tile with corymbs of cream florets over the outer tiles' leaves."""
    rgba, nrm, rough, ao = _family_tile(px, seed, k, inner)
    if inner:
        return rgba, nrm, rough, ao
    leaf_mean = _opaque_mean(rgba)
    rgba, nrm = rgba.copy(), nrm.copy()
    rough, ao = rough.astype(np.float32).copy(), ao.astype(np.float32).copy()
    rng = random.Random(f"{seed}:blossom:{k}")
    ys, xs = np.mgrid[0:px, 0:px].astype(np.float32)
    x, y = (xs + 0.5) / px, (ys + 0.5) / px
    painted = np.zeros((px, px), bool)
    for _ in range(CLUSTERS[k]):
        cr = rng.uniform(*CLUSTER_R)
        pr, pa = (BLOSSOM_REACH - cr - FLORET_R[1]) * math.sqrt(rng.random()), rng.uniform(0, 2 * math.pi)
        ccx, ccy = 0.5 + pr * math.cos(pa), 0.5 + pr * math.sin(pa)
        florets = []
        for _ in range(rng.randint(11, 16)):
            fr, fa = cr * math.sqrt(rng.random()), rng.uniform(0, 2 * math.pi)
            florets.append((fr / cr, ccx + fr * math.cos(fa), ccy + fr * math.sin(fa)))
        # Back to front: the rim florets first, the crowning ones last and brightest (a domed corymb).
        for d, fx, fy in sorted(florets, reverse=True):
            _floret(rgba, nrm, rough, ao, painted, x, y, fx, fy, rng.uniform(*FLORET_R), rng, 0.72 + 0.28 * (1 - d))
    opaque = rgba[..., 3] > 0.5
    STATS[f"tile_{k}"] = {"clusters": CLUSTERS[k], "blossomFraction": round(float(painted.sum() / opaque.sum()), 4),
                          "leafMeanLinear": leaf_mean, "meanLinear": _opaque_mean(rgba)}
    PAINTED[k] = painted
    FLOWER[k] = (_lin(rgba[..., :3][painted]).sum(axis=0), int(painted.sum()))
    return rgba, nrm, rough, ao


def blossom_orm(size: int, rgba, path: str, non_color: bool = True):
    """`bake.write_png`, with the ORM's A set to 1 - blossom coverage (the season hook's mask),
    laid out as `_shrub.atlas` lays out the tiles. Every other map is written unchanged."""
    if not path.endswith(".orm.png"):
        return _family_write_png(size, rgba, path, non_color=non_color)
    assert size == _shrub.ATLAS and sorted(PAINTED) == [k for k in range(_shrub.LEAF_TILES) if k != _shrub.INNER_TILE]
    alpha = np.ones((size, size), np.float32)
    for k, painted in PAINTED.items():
        x0, y0 = _shrub.tile_origin(k)
        alpha[y0:y0 + _shrub.TILE, x0:x0 + _shrub.TILE] = np.where(painted, 0.0, 1.0)
    orm = np.ascontiguousarray(rgba, np.float32).reshape(-1, 4).copy()
    assert np.all(orm[:, 3] == 1.0), "the family's ORM A is no longer 1"
    orm[:, 3] = alpha.reshape(-1)
    return _family_write_png(size, orm, path, non_color=non_color)


def build(piece: str, out: pathlib.Path, generator: str, *, clusters: tuple[int, ...], **kw) -> None:
    """Install the blossom tile, run the verge-bush pipeline, then add the blossom stats to the bake report."""
    global CLUSTERS
    assert set(kw) == _VB_KW, sorted(kw)
    crown, stem = kw["crown"], kw["stem"]
    assert len(clusters) == _shrub.LEAF_TILES - 1 and all(0 <= c <= 10 for c in clusters), clusters
    assert crown["b"] > crown["a"], "an upright bush: crown b > a"
    assert all(k in crown for k in SPRAY_KEYS), crown
    assert 0 <= crown["sprays"] <= 8 and 0.0 <= crown["spray"] <= 0.35 and 0.2 <= crown["spray_width"] <= 0.8, crown
    # The stems' first kept ring must clear the ground (bush_verge friction 2): sink > climb.
    climb = stem["length"] / 12 * kw["lods"][0]["stride"] * math.sin(math.radians(86))
    assert stem["sink"] > climb, (stem, climb)
    CLUSTERS = dict(enumerate(clusters))
    STATS.clear()
    PAINTED.clear()
    FLOWER.clear()
    _shrub.cluster_tile = blossom_tile
    _shrub.cards = spray_cards
    bake.write_png = blossom_orm
    try:
        _verge_bush.build(piece, out, generator, **kw)
    finally:
        bake.write_png = _family_write_png
    print(f"{piece}: blossom {STATS}")
    report = out / "bake-report.json"
    data = json.loads(report.read_text())
    data["blossomStats"] = STATS
    leaf = np.mean([s["leafMeanLinear"] for s in STATS.values()], axis=0)
    flower = sum(s for s, _ in FLOWER.values()) / sum(n for _, n in FLOWER.values())
    data["blossomHook"] = {"mask": "orm.a = 1 - blossom coverage", "leafLinear": [round(float(v), 4) for v in leaf],
                           "flowerLinear": [round(float(v), 4) for v in flower],
                           "coverage": round(float(sum(p.sum() for p in PAINTED.values()) / _shrub.ATLAS ** 2), 4)}
    report.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
