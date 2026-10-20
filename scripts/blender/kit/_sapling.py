"""Spruce saplings: a young Norway spruce of the forest floor (refs: assets-source/refs/forest, ref-1's
"undergrowth of ferns, shrubs and saplings"), about 2.2 m, a thin bare stem with a few sparse needle
tiers. It is a baby of `tree_forest`: the same bark set, needle tiles, core cone, card shapes, wind
data and pipeline (`_spruce.build`), with sapling proportions.

`_forest.py` and `_spruce.py` back approved pieces (provenance hashes them), so this module edits
neither. It sets `_forest`'s module globals through `_spruce.configure` (the silhouette) and replaces
three `_forest` functions that the pipeline looks up at call time, each a thin wrapper over the
approved one:
- `crown_r`: the original's cone with a 0.08 m tip floor (the original's 0.32 m would make the top
  tier as wide as a sapling's lowest);
- `branches`: the original's whorl list, keeping fillers only on the lowest FILLER_WHORLS whorls, so
  the tiers stay sparse and read as separate;
- `core_mesh`: the original's dark inner cone, narrowed in x and y by CORE_SCALE, so a sparse crown
  shows a thin dark heart and not a solid spike.
`apply` asserts the invariants these rely on (the original signatures and the subset relation)."""
from __future__ import annotations

import inspect
import pathlib

import _forest as F
import _spruce

TIP_FLOOR = 0.08  # crown radius (m) at the top whorl
FILLER_WHORLS = 2  # whorls (from the bottom) that keep their filler fronds
CORE_SCALE = 0.3  # the inner cone's radius as a fraction of the original's 0.36 x crown_r
CORE_TIP, CORE_FOOT = 0.12, 0.08  # the cone's tip above / foot below its rings (original 0.6, 0.25)
LEADER_SCALE = 0.45  # leader card width as a fraction of the original's

_ORIG_CROWN_R = F.crown_r
_ORIG_BRANCHES = F.branches
_ORIG_CORE = F.core_mesh
_ORIG_CARDS = F.card_mesh


def crown_r(z: float) -> float:
    f = min(max((z - F.Z0) / (F.ZTOP - F.Z0), 0.0), 1.0)
    return F.RMAX * (1 - f) ** 0.85 + TIP_FLOOR


def branches() -> list[dict]:
    """The approved whorl list with fillers kept only on the lowest FILLER_WHORLS whorls (ids kept,
    so every card's wind phase is the original's for that card)."""
    cards = _ORIG_BRANCHES()
    whorl_z = sorted({round(b["z"], 6) for b in cards if b["kind"] == "main"})
    keep_z = whorl_z[FILLER_WHORLS - 1] + 0.3
    out = [b for b in cards if b["kind"] == "main" or b["z"] <= keep_z]
    assert all(b in cards for b in out) and sum(b["kind"] == "main" for b in out) == sum(
        b["kind"] == "main" for b in cards)
    return out


def core_mesh(name: str, sides: int, rings: int, uv_box: tuple[float, float, float, float]):
    """The approved cone, narrowed by CORE_SCALE, its 0.6 m tip spike (sized for a 16 m tree) cut to
    CORE_TIP and its bottom point to CORE_FOOT, so it stays inside a sapling's crown."""
    obj = _ORIG_CORE(name, sides, rings, uv_box)
    za, zb = F.Z0 + 0.25, F.ZTOP - 0.3
    tips = 0
    for v in obj.data.vertices:
        v.co.x *= CORE_SCALE
        v.co.y *= CORE_SCALE
        if abs(v.co.z - (zb + 0.6)) < 1e-5:
            v.co.z, tips = zb + CORE_TIP, tips + 1
        elif abs(v.co.z - (za - 0.25)) < 1e-5:
            v.co.z = za - CORE_FOOT
    assert tips == 1, tips
    obj.data.update()
    return obj


def card_mesh(name: str, cards: list[dict], segs: int, folded: bool, width_scale: float = 1.0, leader: int = 2):
    """The approved fronds, plus the approved leader cards narrowed to LEADER_SCALE of their width
    (the original's 0.17 m half width is a 16 m tree's top shoot)."""
    from lib import geom

    fronds = _ORIG_CARDS(f"{name}_fronds", cards, segs, folded, width_scale, leader=0)
    if not leader:
        fronds.name = fronds.data.name = name
        return fronds
    lead = _ORIG_CARDS(f"{name}_leader", [], segs, folded, width_scale, leader=leader)
    for v in lead.data.vertices:
        assert abs(v.co.xy.length - 0.17) < 1e-4, v.co
        v.co.x *= LEADER_SCALE
        v.co.y *= LEADER_SCALE
    lead.data.update()
    obj = geom.join([fronds, lead], name)
    obj.data.uv_layers.active = obj.data.uv_layers["UVMap"]
    assert [u.name for u in obj.data.uv_layers] == ["UVMap", F.WIND_UV], [u.name for u in obj.data.uv_layers]
    return obj


def apply() -> None:
    """Install the wrappers on `_forest`, after checking the approved functions they wrap."""
    assert list(inspect.signature(_ORIG_CORE).parameters) == ["name", "sides", "rings", "uv_box"]
    assert not inspect.signature(_ORIG_BRANCHES).parameters
    assert list(inspect.signature(_ORIG_CROWN_R).parameters) == ["z"]
    assert list(inspect.signature(_ORIG_CARDS).parameters) == ["name", "cards", "segs", "folded", "width_scale",
                                                                "leader"]
    for k in ("Z0", "ZTOP", "RMAX", "WHORLS", "HEIGHT", "TRUNK_R0", "TRUNK_R1", "WIND_UV"):
        assert hasattr(F, k), k
    F.crown_r = crown_r
    F.branches = branches
    F.core_mesh = core_mesh
    F.card_mesh = card_mesh


def build(piece: str, out: pathlib.Path, generator: str, *, shape: dict, flare: float, bark_uv_per_m: float,
          lods: tuple[dict, ...]) -> None:
    """`_spruce.build` with the sapling wrappers installed."""
    apply()
    _spruce.build(piece, out, generator, shape=shape, flare=flare, bark_uv_per_m=bark_uv_per_m, lods=lods)
