"""Matte leaves for the woodland shrubs shrub_forest (hazel) and shrub_forest_c (bramble), owner A23 Q2
(2026-10-26): their leaf texels drew at the family's roughness 0.50-0.64 (`_shrub.cluster_tile`), which
reads as a glossy grey sheen across the dome in play, as on the verge bushes before e379d55
(`docs/checkpoints/bush-shine/kit-eye-after-crop.jpg`).

Pipeline: the family build (`_shrub.build` or `_shrub_bramble.build`) runs unchanged, with
`_shrub.cluster_tile` wrapped for its duration so the ORM's G goes through `_verge_bush.matte`: texels
under 0.7 (leaves, twigs and the cleared background) map linearly onto 0.76-0.86, the verge bushes'
range; the shade blob (0.85) keeps its value. Albedo, normal, AO, masks, cards, stems, wind, collider
and export are the family's, byte for byte. `_shrub.py` and `_verge_bush.py` are not edited: the holly
(shrub_forest_b, glossy on purpose) finds its leaves by roughness under 0.7 in `_shrub_holly.py`.

Invariants this module relies on (asserted below, so a change to `_shrub` fails here, not in play):
`_shrub.atlas` looks `cluster_tile` up by global name, and its signature."""
from __future__ import annotations

import inspect
import pathlib
from collections.abc import Callable

import _shrub
import _verge_bush

assert "cluster_tile" in _shrub.atlas.__code__.co_names, "_shrub.atlas no longer looks cluster_tile up by name"
assert list(inspect.signature(_shrub.cluster_tile).parameters) == ["px", "seed", "k", "inner"], \
    inspect.signature(_shrub.cluster_tile)
assert _verge_bush.LEAF_ROUGHNESS == (0.76, 0.86), _verge_bush.LEAF_ROUGHNESS


def build(family_build: Callable[..., None], piece: str, out: pathlib.Path, generator: str, **kw) -> None:
    """Run `family_build` with every keyword passed through and the leaf roughness made matte."""
    tile = _shrub.cluster_tile

    def matte_tile(px: int, seed: str, k: int, inner: bool) -> tuple:
        rgba, nrm, rough, ao = tile(px, seed, k, inner)
        return rgba, nrm, _verge_bush.matte(rough), ao

    _shrub.cluster_tile = matte_tile
    try:
        family_build(piece, out, generator, **kw)
    finally:
        _shrub.cluster_tile = tile
