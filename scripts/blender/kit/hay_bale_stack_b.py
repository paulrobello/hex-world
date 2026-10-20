"""hay_bale_stack_b: a smaller stack of three twine-bound rectangular straw bales, two side by side with
one laid across them on top, about 1.0 x 1.0 m and 0.7 m high (refs: assets-source/refs/farm, the small
bale stacks in the farmyard). A variant of hay_bale_stack (PRD §6 Variety); the pipeline is
`_hay.build_stack` driven by `_hay_var.build_stack_b`. Scripted route, prop kind (LOD0 <= 1,500 tris):
make kit-build PIECE=hay_bale_stack_b.

The pivot is on the ground under the stack's middle; the lower bales' long axis runs along Blender X
(runtime x), the top bale's across it. The bottom row is sunk 3 cm. Straw does not sway: no wind data."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _hay_var  # noqa: E402

_hay_var.build_stack_b("hay_bale_stack_b", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__)
