"""river_reeds_b: a stand of two reed clumps with cattails, 0.85 m apart, a riverbank scatter prop (owner, 2026-10-26: rocks and reeds are scatter,
not baked into the tile). Refs: assets-source/refs/river. Scripted route, prop kind (LOD0 <=
1,500 tris): make kit-build PIECE=river_reeds_b. Pipeline: `_bank_props`."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _bank_props  # noqa: E402

_bank_props.build("river_reeds_b", __file__, reeds=[(-0.4, 0.15, 1907), (0.4, -0.15, 4426)])
