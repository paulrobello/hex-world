"""beach_pebbles_c: a loose cluster of seven smaller pebbles for the beach's dry sand at the grass
line (refs: assets-source/refs/beach ref-1, approved 2026-10-13). A scatter prop (owner 2026-10-26). Scripted route, prop kind (LOD0 <= 1,500
tris): make kit-build PIECE=beach_pebbles_c. Pipeline, sizes and LODs: `_stones`."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _stones  # noqa: E402

_stones.build("beach_pebbles_c", __file__)
