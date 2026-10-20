"""Holly-like forest shrub (refs: assets-source/refs/forest, ref-1's "undergrowth of ferns, shrubs and
saplings"): the pine forest's second shrub, a dark glossy blue-green evergreen, taller than it is
wide, an ogive cone that is broadest low down and closes to a rounded point, with a few clusters of
red berries on its outer leaves. It must not read as shrub_forest's open hazel dome, as the
box-green round verge bushes, or as a spruce sapling (a thin open cone of needles).

Pipeline: `_shrub.build` runs unchanged, driven through its module globals (the `_verge_bush_b.py`
pattern), so the cards, stems, wind data, maps, collider and export are the family's and `_shrub.py`
is not edited. What this family changes:
- the palette (`_shrub.DARK`, `MID`, `LIGHT`, `BACK`, `TWIG`), read by `cluster_tile` at call time;
- `cluster_tile` is wrapped (`holly_tile`): on the outer tiles the leaf texels get a low roughness
  (glossy holly) and seeded clusters of red berries are painted over them (alpha only grows, so the
  per-tile coverage cannot drop); the inner tile and the shade blob stay matte;
- `cards` is wrapped (`ogive_cards`): every card centre moves from the family's ellipsoid onto an
  ogive profile R(t) = min(1, flare + (1 - flare) * t / T_FULL) * (1 - t ** OGIVE_POW) ** ogive
  over t = (zf + 1) / 2, and each outer card faces the ogive's own outward normal (with the family's
  up bias). Crown keys `flare` (radius fraction at the bottom) and `ogive` (top closing exponent).

Invariants this module relies on (asserted below, so a change to `_shrub` fails here, not in play):
`_shrub.build` looks `cards` up by global name and `_shrub.atlas` looks `cluster_tile` up by global
name, their signatures, the 1024 atlas with tile 3 as the inner tile, and `_shrub.build` takes every
value as a keyword with no defaults (module globals never reach a default argument)."""
from __future__ import annotations

import inspect
import math
import pathlib
import random

import numpy as np
from mathutils import Vector

import _shrub

assert "cards" in _shrub.build.__code__.co_names, "_shrub.build no longer looks cards up by name"
assert "cluster_tile" in _shrub.atlas.__code__.co_names, "_shrub.atlas no longer looks cluster_tile up by name"
assert list(inspect.signature(_shrub.cards).parameters) == ["seed", "crown"], inspect.signature(_shrub.cards)
assert list(inspect.signature(_shrub.cluster_tile).parameters) == ["px", "seed", "k", "inner"], \
    inspect.signature(_shrub.cluster_tile)
assert (_shrub.ATLAS, _shrub.TILE, _shrub.LEAF_TILES, _shrub.INNER_TILE) == (1024, 480, 4, 3)
_BUILD_KW = {"crown", "stem", "lods", "height", "reach_range", "core_r", "core_h"}
_build_params = inspect.signature(_shrub.build).parameters
assert {n for n, p in _build_params.items() if p.kind is p.KEYWORD_ONLY} == _BUILD_KW, list(_build_params)
assert all(p.default is p.empty for p in _build_params.values()), "a _shrub.build default would be bypassed"
PALETTE_NAMES = ("DARK", "MID", "LIGHT", "BACK", "TWIG")
OGIVE_KEYS = ("flare", "ogive")
T_FULL = 0.2  # t at which the ogive reaches its full radius
OGIVE_POW = 1.6
BERRY = np.array((0xb3, 0x14, 0x16), np.float32) / 255  # sRGB holly red
BERRY_DARK = np.array((0x5c, 0x08, 0x0a), np.float32) / 255

_family_cards = _shrub.cards
_family_tile = _shrub.cluster_tile


def srgb(hex_str: str) -> np.ndarray:
    return np.array([int(hex_str[i:i + 2], 16) for i in (1, 3, 5)], np.float32) / 255


def profile(t: float, crown: dict) -> float:
    """Ogive radius fraction at t in [0, 1] (0 the crown's bottom, 1 its top)."""
    t = min(1.0, max(0.0, t))
    return min(1.0, crown["flare"] + (1 - crown["flare"]) * t / T_FULL) * (1 - t ** OGIVE_POW) ** crown["ogive"]


def ogive_cards(seed: str, crown: dict) -> list[dict]:
    """The family's cards moved from the ellipsoid onto the ogive, outer cards facing out from it."""
    cs = _family_cards(seed, crown)
    assert all({"p", "n", "kind"} <= set(c) for c in cs), "card dict keys changed"
    cx, cz, a, b = crown["cx"], crown["cz"], crown["a"], crown["b"]
    for c in cs:
        d = c["p"] - Vector((cx, 0.0, cz))
        zf = max(-1.0, min(1.0, d.z / b))
        t = (zf + 1) / 2
        ell = math.sqrt(max(0.09, 1 - zf * zf))
        f = profile(t, crown) / ell
        c["p"] = Vector((cx + d.x * f, d.y * f, cz + d.z))
        if c["kind"] == "outer":
            h = 0.01
            drdz = a * (profile(t + h, crown) - profile(t - h, crown)) / (2 * h) / (2 * b)
            az = math.atan2(d.y, d.x)
            n = Vector((math.cos(az), math.sin(az), -drdz)).normalized()
            c["n"] = (n + Vector((0, 0, 0.25))).normalized()
    return cs


def holly_tile(px: int, seed: str, k: int, inner: bool) -> tuple[np.ndarray, ...]:
    """The family's cluster tile, made glossy and dotted with berry clusters on the outer tiles."""
    rgba, nrm, rough, ao = _family_tile(px, seed, k, inner)
    if inner:
        return rgba, nrm, rough, ao
    leaf = (rgba[..., 3] > 0.5) & (rough < 0.7)
    rough = np.where(leaf, np.clip(rough * 0.62, GLOSS_MIN, 1.0), rough).astype(np.float32)
    rng = random.Random(f"{seed}:berries:{k}")
    ys, xs = np.mgrid[0:px, 0:px].astype(np.float32)
    x, y = (xs + 0.5) / px, (ys + 0.5) / px
    for _ in range(BERRY_CLUSTERS):
        pr, pa = 0.32 * math.sqrt(rng.random()), rng.uniform(0, 2 * math.pi)
        ccx, ccy = 0.5 + pr * math.cos(pa), 0.5 + pr * math.sin(pa)
        for _ in range(rng.randint(3, 5)):
            bx, by = ccx + rng.uniform(-0.03, 0.03), ccy + rng.uniform(-0.03, 0.03)
            r = BERRY_R * rng.uniform(0.85, 1.1)
            dx, dy = (x - bx) / r, (y - by) / r
            dd = dx * dx + dy * dy
            on = dd < 1.0
            nz = np.sqrt(np.clip(1 - dd, 0, 1))
            shade = np.clip(0.55 + 0.45 * (0.5 * dx + 0.6 * dy + nz) / 1.4, 0, 1)
            col = BERRY_DARK + (BERRY - BERRY_DARK) * shade[..., None]
            col = np.where(((dx + 0.35) ** 2 + (dy + 0.35) ** 2 < 0.06)[..., None], np.minimum(col + 0.25, 1), col)
            rgba[..., :3] = np.where(on[..., None], col, rgba[..., :3])
            rgba[..., 3] = np.where(on, 1.0, rgba[..., 3])
            nrm[:] = np.where(on[..., None], np.stack([dx, dy, nz], -1) * 0.5 + 0.5, nrm)
            rough = np.where(on, 0.32, rough).astype(np.float32)
            ao = np.where(on, np.maximum(ao, 0.85), ao).astype(np.float32)
    return rgba, nrm, rough, ao


BERRY_CLUSTERS = 5
BERRY_R = 0.014  # tile units: about 1.1 cm on a 0.38 m card
GLOSS_MIN = 0.34  # pitfalls: a dark dielectric much glossier than this reads as grey plastic


def build(piece: str, out: pathlib.Path, generator: str, *, palette: dict, berries: int, **kw) -> None:
    """Set the palette and berry count, install the ogive cards and holly tiles, then run `_shrub.build`."""
    global BERRY_CLUSTERS
    assert set(kw) == _BUILD_KW, sorted(kw)
    assert set(palette) == set(PALETTE_NAMES), palette
    crown, stem = kw["crown"], kw["stem"]
    assert all(k in crown for k in OGIVE_KEYS), crown
    assert 0.5 <= crown["flare"] <= 1.0 and 0.2 <= crown["ogive"] <= 1.0, crown
    assert 0 <= berries <= 12, berries
    # Stems stay inside the crown and the collider inside the leaves (the walker stops at the leaves).
    assert stem["length"] < crown["cz"] and kw["core_r"] < crown["a"], (stem, crown, kw["core_r"])
    for name, value in palette.items():
        setattr(_shrub, name, srgb(value))
    BERRY_CLUSTERS = berries
    _shrub.cards = ogive_cards
    _shrub.cluster_tile = holly_tile
    _shrub.build(piece, out, generator, **kw)
