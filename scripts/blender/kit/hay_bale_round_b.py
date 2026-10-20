"""hay_bale_round_b: a net-wrapped round straw bale stood on its flat end, 1.5 m across and about 1.15 m
high, settled a little (a flattened, bulging foot and a slight lean) with loose straw round its foot
(refs: assets-source/refs/farm, the round bales in a yard scattered with loose straw). A variant of
hay_bale_round (PRD §6 Variety), on the same lathe profile, straw and atlas layout; the pipeline is
`_hay_var.build_round_b` over `_hay`'s builders. Scripted route, prop kind (LOD0 <= 1,500 tris):
make kit-build PIECE=hay_bale_round_b.

The pivot is on the ground under the bale's axis; the axis is vertical. The foot is sunk 6 cm and the
straw apron drapes on the meadow relief of the farm's round-bale slots. Straw does not sway: no wind
data."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _hay_var  # noqa: E402

_hay_var.build_round_b("hay_bale_round_b", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__)
