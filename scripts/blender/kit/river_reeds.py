"""river_reeds: one reed clump with cattails, as the river centres drew them, a riverbank scatter prop (owner, 2026-10-26: rocks and reeds are scatter,
not baked into the tile). Refs: assets-source/refs/river. Scripted route, prop kind (LOD0 <=
1,500 tris): make kit-build PIECE=river_reeds. Pipeline: `_bank_props`."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _bank_props  # noqa: E402

_bank_props.build("river_reeds", __file__, reeds=[(0.0, 0.0, 731)])
