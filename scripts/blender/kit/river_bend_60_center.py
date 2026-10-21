"""river_bend_60_center: bank stones at the waterline and reed clumps on both banks of the tight 7.42 m
bend (river_bend_60 sits on river_bend_60_base, which carries the bed), thinned on the short inner
bank. Scripted route (PRD section 11.1). Layout and contract: _river_dress.py, _river_bend_60_dress.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _river_bend_60_dress as dress  # noqa: E402
import _river_dress  # noqa: E402

PIECE = "river_bend_60_center"
root, lods, out, stats = _river_dress.build_dressing(PIECE, "bend60", __file__, skip=dress.inner_skip)
_river_dress.finish_dressing(root, lods, out, PIECE, __file__, stats)
