"""Road-verge bushes (refs: assets-source/refs/road_straight, ref-1's "grass verges with tufts and small
bushes"): small, dense, round bushes in the grass beyond the kerb, a compact clipped-looking mass of
small dark leaves with no stems showing, unlike shrub_forest's open hazel dome on arching stems.

Pipeline: `_shrub.build` runs unchanged, driven through its module globals (the `_house_jettied.py`
over `_house_longhouse` pattern), so the numpy leaf-cluster atlas, the card and stem meshes, the
wind data, the maps, the collider and the export are the family's, and `_shrub.py` is not edited.
What changes per piece:
- the palette (`_shrub.DARK`, `MID`, `LIGHT`, `BACK`, `TWIG`), set before `build` draws the atlas:
  darker and bluer than the hazel, so the verge bush reads as box or privet, not woodland;
- the crown: a smaller ellipsoid with more, smaller cards (smaller leaves on screen), cards running
  down to the ground (`zmin`) so the mass sits on the grass, and a large inner fill so gaps read as
  shade;
- the stems: three short ones hidden inside the lower dome (the manifest keeps the stems slot, so
  every LOD still carries the primitive);
- the collider: an octagonal prism over most of the mass (a clipped bush is solid to the walker).

Invariants this module relies on (asserted below, so a change to `_shrub` fails here, not in play):
the 1024 atlas of four 480 px tiles over a 64-row stem strip, tile 3 as the inner tile, and the
palette names `cluster_tile` reads at call time."""
from __future__ import annotations

import pathlib

import numpy as np

import _shrub

assert (_shrub.ATLAS, _shrub.TILE, _shrub.STEM_ROWS) == (1024, 480, 64), (_shrub.ATLAS, _shrub.TILE)
assert (_shrub.LEAF_TILES, _shrub.INNER_TILE) == (4, 3), (_shrub.LEAF_TILES, _shrub.INNER_TILE)
PALETTE_NAMES = ("DARK", "MID", "LIGHT", "BACK", "TWIG")
for _name in PALETTE_NAMES:
    assert getattr(_shrub, _name).shape == (3,), _name


def srgb(hex_str: str) -> np.ndarray:
    return np.array([int(hex_str[i:i + 2], 16) for i in (1, 3, 5)], np.float32) / 255


def build(piece: str, out: pathlib.Path, generator: str, *, palette: dict, crown: dict, stem: dict,
          lods: tuple[dict, ...], height: tuple[float, float], reach_range: tuple[float, float], core_r: float,
          core_h: float) -> None:
    """Set the palette, then run the shrub pipeline with this piece's crown, stems and LODs."""
    assert set(palette) == set(PALETTE_NAMES), palette
    for name, value in palette.items():
        setattr(_shrub, name, srgb(value))
    # The stems stay inside the lower dome: their tips below the crown centre, their reach inside it.
    assert stem["length"] < crown["cz"] and stem["count"] <= 3, (stem, crown)
    assert core_r < crown["a"] and core_h < crown["cz"] + crown["b"], (core_r, core_h, crown)
    _shrub.build(piece, out, generator, crown=crown, stem=stem, lods=lods, height=height,
                 reach_range=reach_range, core_r=core_r, core_h=core_h)
