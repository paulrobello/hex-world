"""hay_bale_stack: a small stack of five twine-bound rectangular straw bales, three side by side with two
on top, about 1.0 x 1.4 m and 0.7 m high (refs: assets-source/refs/farm, the bale stacks in the
farmyard). The pipeline is `_hay.build_stack`. Scripted route, prop kind (LOD0 <= 1,500 tris):
make kit-build PIECE=hay_bale_stack.

The pivot is on the ground under the stack's middle; the bales' long axis runs along Blender X
(runtime x). The bottom row is sunk 3 cm. Straw does not sway: no wind data."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _hay  # noqa: E402

_hay.build_stack("hay_bale_stack", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__)
