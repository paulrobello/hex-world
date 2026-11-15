"""rail_viaduct_l1: the stone rail viaduct with its deck one level (4 m) over the tile (`_rail_viaduct`)."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _rail_viaduct

_rail_viaduct.build_piece(1, __file__)
