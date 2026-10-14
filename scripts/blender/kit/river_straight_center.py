"""river_straight_center: bank stones at the waterline and reed clumps on both banks of the straight
channel (river_straight sits on river_straight_base, which carries the bed). Scripted route
(PRD section 11.1). Layout and contract: _river.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _river_dress  # noqa: E402

PIECE = "river_straight_center"
root, lods, out, stats = _river_dress.build_dressing(PIECE, "straight", __file__)
_river_dress.finish_dressing(root, lods, out, PIECE, __file__, stats)
