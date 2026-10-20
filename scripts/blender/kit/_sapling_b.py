"""Second spruce sapling (refs: assets-source/refs/forest, ref-1's "undergrowth of ferns, shrubs and
saplings"): a shorter, bushier young spruce than `sapling_spruce`, with a wide full cone of level
tiers, a filler frond on every whorl and a leader bent to one side, so the undergrowth's saplings
are not all one thin tiered whip.

Pipeline: `_sapling.build` over `_spruce.build`, both unchanged and driven through module globals
(`_sapling.FILLER_WHORLS`, `CORE_SCALE`, `CORE_TIP`, `CORE_FOOT` and the `_forest` silhouette through
`shape`). What this family adds, by module-attribute replacement before `_sapling.apply` installs
its wrappers on `_forest`:
- `branches`: `_sapling`'s whorl list with each card's droop scaled by DROOP_SCALE (a young
  spruce's branches stand level, and `_spruce.build` keeps every needle card above 0.6 m);
- `bend`: the leader bent sideways, an offset BEND x s^2 along BEND_DIR, s rising from 0 at BEND_Z0
  to 1 at the leader tip, applied alike to the trunk skeleton (`_spruce.trunk_skeleton`), the frond
  and leader cards and the core cone, so every part stays attached.

Invariants this module relies on (asserted below, so a change to the helpers fails here, not in
play): `_spruce.build` calls `trunk_skeleton` by global name, `_sapling.apply` installs `_sapling`'s
own `branches`, `core_mesh` and `card_mesh` globals, and the wrapped signatures. Module globals do
not reach default arguments, so every call passes every argument."""
from __future__ import annotations

import inspect
import math
import pathlib

import bpy

import _forest as F
import _sapling
import _spruce

assert "trunk_skeleton" in _spruce.build.__code__.co_names, "_spruce.build no longer looks trunk_skeleton up"
for _name in ("branches", "core_mesh", "card_mesh"):
    assert f"F.{_name} = {_name}" in inspect.getsource(_sapling.apply), _name
assert list(inspect.signature(_spruce.trunk_skeleton).parameters) == ["flare"]
assert list(inspect.signature(_sapling.card_mesh).parameters) == ["name", "cards", "segs", "folded", "width_scale",
                                                                   "leader"]
assert list(inspect.signature(_sapling.core_mesh).parameters) == ["name", "sides", "rings", "uv_box"]
for _k in ("FILLER_WHORLS", "CORE_SCALE", "CORE_TIP", "CORE_FOOT", "TIP_FLOOR", "LEADER_SCALE"):
    assert hasattr(_sapling, _k), _k

_SAP_BRANCHES = _sapling.branches
_SAP_CORE = _sapling.core_mesh
_SAP_CARDS = _sapling.card_mesh
_SPRUCE_TRUNK = _spruce.trunk_skeleton

DROOP_SCALE = 0.5
BEND = 0.12  # sideways offset (m) of the leader tip
BEND_DIR = math.radians(25)
BEND_Z0 = 1.2  # height (m) where the bend starts


def offset(z: float) -> tuple[float, float]:
    top = F.HEIGHT + 0.25
    s = min(max((z - BEND_Z0) / (top - BEND_Z0), 0.0), 1.0)
    d = BEND * s * s
    return d * math.cos(BEND_DIR), d * math.sin(BEND_DIR)


def _bend_obj(obj: bpy.types.Object) -> bpy.types.Object:
    for v in obj.data.vertices:
        dx, dy = offset(v.co.z)
        v.co.x += dx
        v.co.y += dy
    obj.data.update()
    return obj


def branches() -> list[dict]:
    out = []
    for b in _SAP_BRANCHES():
        c = dict(b)
        c["droop"] = b["droop"] * DROOP_SCALE
        out.append(c)
    return out


def trunk_skeleton(flare: float) -> list[dict]:
    skel = _SPRUCE_TRUNK(flare)
    assert len(skel) == 1
    for p in skel[0]["points"]:
        dx, dy = offset(p.z)
        p.x += dx
        p.y += dy
    return skel


def core_mesh(name: str, sides: int, rings: int, uv_box: tuple[float, float, float, float]):
    obj = _SAP_CORE(name, sides, rings, uv_box)
    zs = sorted({round(v.co.z, 5) for v in obj.data.vertices})
    assert len(zs) == rings + 3 and zs == sorted(zs), zs  # foot, rings + 1 levels, tip: ascending
    print(f"{F.PIECE}: core {name} z levels {zs}")
    return _bend_obj(obj)


def card_mesh(name: str, cards: list[dict], segs: int, folded: bool, width_scale: float = 1.0, leader: int = 2):
    return _bend_obj(_SAP_CARDS(name, cards, segs, folded, width_scale, leader))


def build(piece: str, out: pathlib.Path, generator: str, *, shape: dict, flare: float, bark_uv_per_m: float,
          lods: tuple[dict, ...], filler_whorls: int, core: dict, lod0_max: int) -> None:
    """`_sapling.build` with this family's wrappers installed, then the checks `_spruce.build` leaves
    to the caller: LOD0 under lod0_max, and wind U <= 0.1 on every vertex below 0.5 m (the gate's
    still band) in every LOD."""
    _sapling.FILLER_WHORLS = filler_whorls
    _sapling.CORE_SCALE = core["scale"]
    _sapling.CORE_TIP = core["tip"]
    _sapling.CORE_FOOT = core["foot"]
    _sapling.branches = branches
    _sapling.core_mesh = core_mesh
    _sapling.card_mesh = card_mesh
    _spruce.trunk_skeleton = trunk_skeleton
    _sapling.build(piece, out, generator, shape=shape, flare=flare, bark_uv_per_m=bark_uv_per_m, lods=lods)
    assert F.branches is branches and F.core_mesh is core_mesh and F.card_mesh is card_mesh
    from lib import geom

    for i in range(3):
        o = bpy.data.objects[f"{piece}_lod{i}"]
        wl = o.data.uv_layers[F.WIND_UV].data
        low = max((wl[li].uv[0] for li, lp in enumerate(o.data.loops) if o.data.vertices[lp.vertex_index].co.z < 0.5),
                  default=0.0)
        top = max(v.co.z for v in o.data.vertices)
        print(f"{piece}: lod{i} tris {geom.tri_count(o)}, max wind U below 0.5 m {low:.3f}, top z {top:.3f}")
        assert low <= 0.1, (i, low)
        if i == 0:
            assert geom.tri_count(o) <= lod0_max, geom.tri_count(o)
