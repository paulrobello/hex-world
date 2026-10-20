"""Bramble-mound forest shrub (refs: assets-source/refs/forest, ref-1's "undergrowth of ferns, shrubs and
saplings"): the pine forest's third shrub, a low, wide, lumpy mound of fresh yellow-green leaves with
reddish arching canes that climb out of its top and bow back down past its rim. It must not read as
shrub_forest's round hazel dome (taller, mid green), shrub_forest_b's dark holly cone, a fern clump
(fronds, no woody canes) or a spruce sapling. It stays well below the spruces' lowest whorl (1.95 m),
so it fits under their crowns (owner Q46), and its palette is lighter than the hazel's so it reads in
spruce shade (owner Q60). No berries or flowers (owner Q61: seasonal paint would show all year).

Pipeline: `_shrub.build` runs unchanged, driven through its module globals (the `_shrub_holly.py`
pattern), so the cards, card mesh, stems mesh, wind data, maps, collider and export are the family's
and `_shrub.py` is not edited. What this family changes:
- the palette (`_shrub.DARK`, `MID`, `LIGHT`, `BACK`, `TWIG`), read by `cluster_tile` at call time;
- `stems` is replaced (`cane_stems`): each cane (a cubic Bezier in its own vertical plane) rises
  nearly upright from the base, crests just over the mound and drops outward past the leaf mass, its
  tip held at least CANE_TIP_MIN above the litter, so the canes' outer bows read against the litter
  and none roots (wind U stays a free-tip sway);
- `cards` is wrapped (`mound_cards`): the family's ellipsoid cards move onto a lumpy mound (seeded
  low-order azimuthal lobes in radius and height plus `bumps` seeded sub-mounds and dents on the
  upper shell, so every yaw shows a different, broken outline), and small cards ride each cane's
  outer bow (CANE_CARDS per cane, alternating sides), so the canes carry leaves. Crown keys
  `lobes` (radius lobe amplitude), `hump` (height lobe amplitude) and `bumps` (sub-mound count).

Invariants this module relies on (asserted below, so a change to `_shrub` fails here, not in play):
`_shrub.build` looks `cards` and `stems` up by global name, their signatures, the stem dict keys
`stem_mesh` reads (`pts`, `radii`, `phase`), the 1024 atlas with tile 3 as the inner tile, and
`_shrub.build` takes every value as a keyword with no defaults (module globals never reach a
default argument)."""
from __future__ import annotations

import inspect
import math
import pathlib
import random

import numpy as np
from mathutils import Vector

import _shrub

assert "cards" in _shrub.build.__code__.co_names, "_shrub.build no longer looks cards up by name"
assert "stems" in _shrub.build.__code__.co_names, "_shrub.build no longer looks stems up by name"
assert list(inspect.signature(_shrub.cards).parameters) == ["seed", "crown"], inspect.signature(_shrub.cards)
assert list(inspect.signature(_shrub.stems).parameters) == ["seed", "spec"], inspect.signature(_shrub.stems)
assert (_shrub.ATLAS, _shrub.TILE, _shrub.LEAF_TILES, _shrub.INNER_TILE) == (1024, 480, 4, 3)
_STEM_KEYS = {"pts", "radii", "phase"}
assert all(f'st["{k}"]' in inspect.getsource(_shrub.stem_mesh) for k in _STEM_KEYS), "stem_mesh keys changed"
_BUILD_KW = {"crown", "stem", "lods", "height", "reach_range", "core_r", "core_h"}
_build_params = inspect.signature(_shrub.build).parameters
assert {n for n, p in _build_params.items() if p.kind is p.KEYWORD_ONLY} == _BUILD_KW, list(_build_params)
assert all(p.default is p.empty for p in _build_params.values()), "a _shrub.build default would be bypassed"
PALETTE_NAMES = ("DARK", "MID", "LIGHT", "BACK", "TWIG")
MOUND_KEYS = ("lobes", "hump", "bumps")
CANE_KEYS = ("count", "apex", "reach", "tip_z", "r0", "r1", "sink")
CANE_STEPS = 12  # skeleton points per cane = CANE_STEPS + 1 (LOD strides 2, 4, 6 all land on the tip)
CANE_TIP_MIN = 0.18  # m above the litter: tips stay free, well above their radius
CANE_CARDS = 3  # small leaf cards along each cane's outer bow
CANE_CARD_SIZE = 0.5  # fraction of the crown card size

_family_cards = _shrub.cards
_SPEC: dict = {}


def srgb(hex_str: str) -> np.ndarray:
    return np.array([int(hex_str[i:i + 2], 16) for i in (1, 3, 5)], np.float32) / 255


def lobe_phases(seed: str) -> tuple[float, ...]:
    rng = random.Random(f"{seed}:lobes")
    return tuple(rng.uniform(0, 2 * math.pi) for _ in range(4))


def lobe(az: float, phases: tuple[float, ...]) -> tuple[float, float]:
    """(radius factor shape, height factor shape) in [-1, 1] at azimuth az: two and three lobes in
    radius, one and two in height, so the mound has no symmetry."""
    p1, p2, p3, p4 = phases
    r = 0.6 * math.cos(2 * az + p2) + 0.4 * math.cos(3 * az + p3)
    h = 0.65 * math.cos(az + p1) + 0.35 * math.cos(2 * az + p4)
    return r, h


def cane_stems(seed: str, spec: dict) -> list[dict]:
    """Arching canes: each is a cubic Bezier in its own vertical plane that leaves the base cluster
    nearly upright, crests near `apex` m about 0.5 m out and drops to a hanging tip `tip_z` m above
    the litter at `reach` m; points and radii in the family's stem dict."""
    rng = random.Random(f"{seed}:canes")
    out = []
    n = spec["count"]
    for i in range(n):
        az = 2 * math.pi * (i + rng.uniform(-0.3, 0.3)) / n
        h = Vector((math.cos(az), math.sin(az), 0.0))
        r0 = rng.uniform(0.03, 0.12)
        apex = spec["apex"] * rng.uniform(0.9, 1.08)
        reach = spec["reach"] * rng.uniform(0.9, 1.06)
        tip = spec["tip_z"] * rng.uniform(0.8, 1.2)
        c = apex / 0.75  # a cubic with both inner controls at height c crests near 0.75 c
        ctl = ((r0, -spec["sink"]), (r0 + 0.05, c), (reach * rng.uniform(0.7, 0.8), c), (reach, tip))
        pts = []
        for j in range(CANE_STEPS + 1):
            t, u = j / CANE_STEPS, 1 - j / CANE_STEPS
            w = (u ** 3, 3 * u * u * t, 3 * u * t * t, t ** 3)
            r = sum(wk * p[0] for wk, p in zip(w, ctl))
            z = sum(wk * p[1] for wk, p in zip(w, ctl))
            pts.append(h * r + Vector((0, 0, z)))
        radii = [spec["r0"] + (spec["r1"] - spec["r0"]) * (j / CANE_STEPS) ** 0.7 for j in range(CANE_STEPS + 1)]
        out.append({"pts": pts, "radii": radii, "phase": _shrub.unit(seed, "stem", i)})
    return out


def bumps(seed: str, count: int) -> list[tuple[Vector, float, float]]:
    """Seeded sub-mounds on the upper shell: (direction, amplitude, width); a few are dents."""
    rng = random.Random(f"{seed}:bumps")
    out = []
    for i in range(count):
        az = 2 * math.pi * (i + rng.uniform(-0.35, 0.35)) / count
        el = math.radians(rng.uniform(5, 55))
        d = Vector((math.cos(el) * math.cos(az), math.cos(el) * math.sin(az), math.sin(el)))
        amp = rng.uniform(0.14, 0.26) * (-0.5 if i % 4 == 3 else 1.0)
        out.append((d, amp, rng.uniform(0.12, 0.22)))
    return out


def mound_cards(seed: str, crown: dict) -> list[dict]:
    """The family's cards moved onto the lumpy mound, plus CANE_CARDS small cards along each cane's
    outer bow, alternating sides and facing out from the cane."""
    cs = _family_cards(seed, crown)
    assert all({"p", "n", "kind", "size", "roll", "tile", "phase"} <= set(c) for c in cs), "card dict keys changed"
    cx, cz, a, b = crown["cx"], crown["cz"], crown["a"], crown["b"]
    ph = lobe_phases(seed)
    bs = bumps(seed, crown["bumps"])
    for c in cs:
        d = c["p"] - Vector((cx, 0.0, cz))
        unit_d = Vector((d.x / a, d.y / a, d.z / b)).normalized()
        az = math.atan2(d.y, d.x)
        lr, lh = lobe(az, ph)
        bump = sum(amp * math.exp(-(1 - unit_d.dot(bd)) / w) for bd, amp, w in bs)
        fr = 1.0 + crown["lobes"] * lr + bump
        up = max(0.0, d.z) / b  # humps and bumps lift the upper mound only, the rim stays on the litter
        lift = (crown["hump"] * lh + bump) * up * b
        c["p"] = Vector((cx + d.x * fr, d.y * fr, cz + d.z + lift))
    rng = random.Random(f"{seed}:cane-cards")
    canes = cane_stems(seed, _SPEC)
    for ci, st in enumerate(canes):
        for k in range(CANE_CARDS):
            j = min(CANE_STEPS, round((0.6 + 0.38 * k / max(1, CANE_CARDS - 1)) * CANE_STEPS))
            p = st["pts"][j]
            tang = (st["pts"][min(j + 1, CANE_STEPS)] - st["pts"][j - 1]).normalized()
            out_dir = Vector((p.x, p.y, 0.0)).normalized()
            side = tang.cross(Vector((0, 0, 1))).normalized() * (1 if (ci + k) % 2 else -1)
            n = (out_dir * 0.7 + Vector((0, 0, 0.6)) + side * 0.5).normalized()
            cs.append({"p": p + side * 0.06 + n * 0.02, "n": n,
                       "size": crown["card"] * CANE_CARD_SIZE * rng.uniform(0.85, 1.1),
                       "roll": rng.uniform(0, 6.28), "tile": (ci + k) % (_shrub.LEAF_TILES - 1), "kind": "outer",
                       "phase": _shrub.unit(seed, "cane-card", ci * CANE_CARDS + k)})
    return cs


def build(piece: str, out: pathlib.Path, generator: str, *, palette: dict, **kw) -> None:
    """Set the palette, install the mound cards and the arching canes, then run `_shrub.build`."""
    assert set(kw) == _BUILD_KW, sorted(kw)
    assert set(palette) == set(PALETTE_NAMES), palette
    crown, stem = kw["crown"], kw["stem"]
    assert all(k in crown for k in MOUND_KEYS), crown
    assert set(stem) == set(CANE_KEYS), stem
    assert 0.0 <= crown["lobes"] <= 0.3 and 0.0 <= crown["hump"] <= 0.4, crown
    assert all(l["stride"] and CANE_STEPS % l["stride"] == 0 for l in kw["lods"]), kw["lods"]
    _SPEC.clear()
    _SPEC.update(stem)
    canes = cane_stems(piece, stem)
    for i, st in enumerate(canes):
        zs = [p.z for p in st["pts"]]
        tip = st["pts"][-1]
        print(f"{piece}: cane {i} apex {max(zs):.3f} tip z {tip.z:.3f} at r {math.hypot(tip.x, tip.y):.3f}")
        # Rings above the base clear the litter (U = 0 at z <= 0 holds per loop), tips stay free.
        assert all(z > st["radii"][j] + 0.02 for j, z in enumerate(zs) if j >= 2), (i, zs)
        assert tip.z >= CANE_TIP_MIN, (i, tip)
    # The collider stays inside the leaves and inside the shrub group's widest ground footprint
    # (scatter's group_footprint: shrub_forest's 0.71 m), so no forest tile's mask or spacing moves.
    assert kw["core_r"] < crown["a"] and kw["core_r"] <= 0.70, (crown, kw["core_r"])
    for name, value in palette.items():
        setattr(_shrub, name, srgb(value))
    _shrub.cards = mound_cards
    _shrub.stems = cane_stems
    _shrub.build(piece, out, generator, **kw)
