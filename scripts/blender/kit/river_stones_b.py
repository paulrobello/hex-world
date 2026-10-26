"""river_stones_b: a pair of bank stones, a riverbank scatter prop (owner, 2026-10-26: rocks and reeds are scatter,
not baked into the tile). Refs: assets-source/refs/river. Scripted route, prop kind (LOD0 <=
1,500 tris): make kit-build PIECE=river_stones_b. Pipeline: `_bank_props`."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _bank_props  # noqa: E402

_bank_props.build("river_stones_b", __file__, stones=2)
