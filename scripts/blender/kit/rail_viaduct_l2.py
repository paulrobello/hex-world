"""rail_viaduct_l2: the stone rail viaduct with its deck two levels (8 m) over the tile (`_rail_viaduct`)."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _rail_viaduct  # noqa: E402

_rail_viaduct.build_piece(2, __file__)
