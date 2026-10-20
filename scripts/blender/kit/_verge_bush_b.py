"""Second road-verge bush (refs: assets-source/refs/road_straight, ref-1's "grass verges with tufts and
small bushes"): the smaller, lighter yellow-green, looser bush beside the dark round one, so a verge
reads as two plants, not one bush at two scales.

Pipeline: `_verge_bush.build` over `_shrub.build`, both unchanged and driven through their module
globals. What this family adds is the less regular outline: `_shrub.build` looks `cards` up in its
module globals at call time, so `lumpy_cards` wraps the family's card placer and moves each card's
centre off the ellipsoid by three crown keys, which the bake report records with the crown:
- `lump`: radial amplitude of a seeded low-order wave over the azimuth (two and three lobes plus a
  tilted one), faded out toward the bottom (`LUMP_FLOOR`) so the base still sits on the grass;
- `oval`: the plan's y axis scale (an oval in plan, not a disc);
- `lean`: a shift along x per metre of height above the crown centre (a slight lean of the top).
The card normals, the vertex normals (the family's smooth ellipsoid), the wind, the maps, the
collider and the export stay the family's.

Invariants this module relies on (asserted below, so a change to `_shrub` fails here, not in play):
`_shrub.build` calls `cards` by global name, `cards(seed, crown)` returns dicts with a centre `p`
and a `kind`, and `_verge_bush.build` takes every value as a keyword (module globals do not reach
default arguments, so nothing here relies on a default)."""
from __future__ import annotations

import inspect
import math
import pathlib
import random

from mathutils import Vector

import _shrub
import _verge_bush

assert "cards" in _shrub.build.__code__.co_names, "_shrub.build no longer looks cards up by name"
assert list(inspect.signature(_shrub.cards).parameters) == ["seed", "crown"], inspect.signature(_shrub.cards)
_VB_KW = {"palette", "crown", "stem", "lods", "height", "reach_range", "core_r", "core_h"}
_vb_params = inspect.signature(_verge_bush.build).parameters
assert {n for n, p in _vb_params.items() if p.kind is p.KEYWORD_ONLY} == _VB_KW, list(_vb_params)
assert all(p.default is p.empty for p in _vb_params.values()), "a _verge_bush.build default would be bypassed"
LUMP_KEYS = ("lump", "oval", "lean")
LUMP_FLOOR = -0.45  # zf at and below which the lumps fade to 0 (the base stays on the ellipsoid)

_family_cards = _shrub.cards


def lumpy_cards(seed: str, crown: dict) -> list[dict]:
    """The family's cards, each centre moved off the ellipsoid by the crown's lump, oval and lean."""
    cs = _family_cards(seed, crown)
    assert all({"p", "kind"} <= set(c) for c in cs), "card dict keys changed"
    rng = random.Random(f"{seed}:lumps")
    ph = [rng.uniform(0, 2 * math.pi) for _ in range(4)]
    cx, cz, a, b = crown["cx"], crown["cz"], crown["a"], crown["b"]
    for c in cs:
        d = c["p"] - Vector((cx, 0.0, cz))
        az = math.atan2(d.y, d.x)
        zf = max(-1.0, min(1.0, d.z / b))
        fade = min(1.0, max(0.0, (zf - LUMP_FLOOR) / (0.35 - LUMP_FLOOR)))
        wave = (0.5 * math.cos(2 * az - ph[0]) + 0.35 * math.cos(3 * az - ph[1])
                + 0.3 * math.cos(az - ph[2]) * math.sin(math.pi * (zf + 1) / 2 + ph[3]))
        f = 1.0 + crown["lump"] * wave * fade
        q = Vector((d.x * f, d.y * f * crown["oval"], d.z * (1.0 + 0.5 * crown["lump"] * wave * fade)))
        q.x += crown["lean"] * max(0.0, q.z)
        c["p"] = Vector((cx, 0.0, cz)) + q
    return cs


def build(piece: str, out: pathlib.Path, generator: str, **kw) -> None:
    """Install the lumpy card placer, then run the verge-bush pipeline with every value explicit."""
    assert set(kw) == _VB_KW, sorted(kw)
    crown = kw["crown"]
    assert all(k in crown for k in LUMP_KEYS), crown
    assert 0.0 <= crown["lump"] <= 0.25 and 0.7 <= crown["oval"] <= 1.0 and abs(crown["lean"]) <= 0.3, crown
    _shrub.cards = lumpy_cards
    _verge_bush.build(piece, out, generator, **kw)
