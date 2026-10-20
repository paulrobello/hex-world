"""Third spruce sapling (refs: assets-source/refs/forest, ref-1's "undergrowth of ferns, shrubs and
saplings"): a shade-suppressed young spruce. Beside `sapling_spruce`'s tiered cone and
`sapling_spruce_b`'s short bushy cone it has a long bare stem with a crook in it, and a sparse,
flat-topped crown: three to four tiers of long, narrow, level fronds that stay nearly as wide at the top
as at the bottom (a spruce in deep shade spreads its laterals rather than its leader), a short weak
leader and a thin dark heart.

Pipeline: `_sapling_b.build` over `_sapling.build` over `_spruce.build`, all unchanged and driven
through module globals: `_sapling.TIP_FLOOR` (with a small `RMAX`, the near-equal tier widths),
`_sapling.LEADER_SCALE`, `_sapling_b.DROOP_SCALE`, and two of `_sapling_b`'s call-time lookups:
- `_SAP_BRANCHES`: `_sapling.branches` thinned to every other main card of each whorl (and the filler
  that follows each kept main), ids kept, so the tiers read as sparse sprays;
- `offset`: in place of `_sapling_b`'s leader bend, a crook, a sideways bow BOW x sin^2(pi s) along
  BOW_DIR with s from 0 at BOW_Z0 to 1 at the stem tip, so the stem leaves and rejoins the axis with
  zero slope. The collider is the hull of the trunk rings nearest 0, 1, 2 and 3 m (`_spruce.build`),
  so a bow that is zero at 1 m and at the tip keeps the collider reach, and with it the sapling
  group's scatter footprint (`scatter::group_footprint`), unchanged.

`build` asserts what the group relies on after `_sapling_b.build`: collider reach <= GROUND_MAX (the
group's largest `FOOTPRINTS` ground), crown reach over every LOD <= CROWN_MAX (its largest crown) and
the top below TOP_MAX (sapling_spruce's leader tip, so this variant adds no height under the spruce
crowns, Q46). Module globals do not reach default arguments, so every call passes every argument."""
from __future__ import annotations

import math
import pathlib

import bpy

import _forest as F
import _sapling
import _sapling_b

for _name in ("_SAP_BRANCHES", "offset"):
    assert hasattr(_sapling_b, _name), _name
assert "_SAP_BRANCHES" in _sapling_b.branches.__code__.co_names, "_sapling_b.branches no longer calls _SAP_BRANCHES"
for _fn in (_sapling_b._bend_obj, _sapling_b.trunk_skeleton):
    assert "offset" in _fn.__code__.co_names, f"{_fn.__name__} no longer looks offset up"
for _k in ("TIP_FLOOR", "LEADER_SCALE"):
    assert hasattr(_sapling, _k), _k
assert hasattr(_sapling_b, "DROOP_SCALE")

_SAP_BRANCHES = _sapling.branches

BOW = 0.14  # sideways offset (m) of the crook at its middle
BOW_DIR = math.radians(200)
BOW_Z0 = 1.0  # height (m) where the crook starts; it ends at the stem tip (F.HEIGHT)
GROUND_MAX = 0.1  # sapling group's largest FOOTPRINTS ground (crates/hex-play/src/play/scatter.rs)
CROWN_MAX = 1.1  # and its largest crown
TOP_MAX = 2.45  # sapling_spruce's leader tip (HEIGHT 2.2 + 0.25)


def offset(z: float) -> tuple[float, float]:
    s = min(max((z - BOW_Z0) / (F.HEIGHT - BOW_Z0), 0.0), 1.0)
    d = BOW * math.sin(math.pi * s) ** 2
    return d * math.cos(BOW_DIR), d * math.sin(BOW_DIR)


def branches() -> list[dict]:
    """`_sapling.branches` with every other main card of each whorl and the filler after each kept
    main (fillers follow their main in `_forest.branches`, id + 1)."""
    cards = _SAP_BRANCHES()
    kind = {b["id"]: b["kind"] for b in cards}
    assert all(kind.get(b["id"] - 1) == "main" for b in cards if b["kind"] == "filler"), "filler without its main"
    by_z: dict[float, list[dict]] = {}
    for b in cards:
        if b["kind"] == "main":
            by_z.setdefault(round(b["z"], 6), []).append(b)
    keep = {b["id"] for whorl in by_z.values() for j, b in enumerate(whorl) if j % 2 == 0}
    return [b for b in cards if (b["id"] if b["kind"] == "main" else b["id"] - 1) in keep]


def build(piece: str, out: pathlib.Path, generator: str, *, shape: dict, tip_floor: float, leader_scale: float,
          droop_scale: float, flare: float, bark_uv_per_m: float, lods: tuple[dict, ...], filler_whorls: int,
          core: dict, lod0_max: int) -> None:
    """`_sapling_b.build` with this family's globals, then the group checks (module docstring)."""
    _sapling.TIP_FLOOR = tip_floor
    _sapling.LEADER_SCALE = leader_scale
    _sapling_b.DROOP_SCALE = droop_scale
    _sapling_b._SAP_BRANCHES = branches
    _sapling_b.offset = offset
    _sapling_b.build(piece, out, generator, shape=shape, flare=flare, bark_uv_per_m=bark_uv_per_m, lods=lods,
                     filler_whorls=filler_whorls, core=core, lod0_max=lod0_max)
    assert _sapling_b.offset is offset and _sapling_b._SAP_BRANCHES is branches
    col = bpy.data.objects[f"{piece}_collider"]
    ground = max(math.hypot(*(col.matrix_world @ v.co).xy) for v in col.data.vertices)
    lods_o = [bpy.data.objects[f"{piece}_lod{i}"] for i in range(3)]
    crown = max(math.hypot(v.co.x, v.co.y) for o in lods_o for v in o.data.vertices)
    top = max(v.co.z for o in lods_o for v in o.data.vertices)
    bow = max(math.hypot(*offset(z / 100)) for z in range(0, 300))
    print(f"{piece}: collider reach {ground:.3f} (max {GROUND_MAX}), crown reach {crown:.3f} (max {CROWN_MAX}), "
          f"top {top:.3f} (max {TOP_MAX}), crook {bow:.3f} m")
    assert ground <= GROUND_MAX, ground
    assert crown <= CROWN_MAX, crown
    assert top <= TOP_MAX, top
