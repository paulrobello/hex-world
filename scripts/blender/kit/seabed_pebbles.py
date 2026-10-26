"""seabed_pebbles: a cluster of six dark pebbles on the ocean floor, as ocean_center baked seven
clusters until 2026-10-26 (refs: assets-source/refs/ocean ref-1, approved 2026-10-13). A scatter prop (owner 2026-10-26). Scripted route, prop kind (LOD0 <= 1,500
tris): make kit-build PIECE=seabed_pebbles. Pipeline, sizes and LODs: `_stones`."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _stones  # noqa: E402

_stones.build("seabed_pebbles", __file__)
