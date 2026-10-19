"""hay_bale_round: a net-wrapped round straw bale lying on its side, 1.5 m across and 1.2 m wide (refs:
assets-source/refs/farm, the round bales in the farmyard). The pipeline is `_hay.build_round`.
Scripted route, prop kind (LOD0 <= 1,500 tris): make kit-build PIECE=hay_bale_round.

The pivot is on the ground under the bale's middle; the axis runs along Blender X (runtime x). The
bale is squashed 10% below its axis where it rests and sunk 5 cm, so it sits on the meadow relief
without a visible gap. Straw does not sway: no wind data."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _hay  # noqa: E402

_hay.build_round("hay_bale_round", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__)
