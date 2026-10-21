"""river_fork_center: bank stones at the waterline and reed clumps on the banks of all three arms of the
fork (river_fork sits on river_fork_base, which carries the bed). Scripted route (PRD section 11.1).
Layout: _river_fork.py, _river_fork_dress.py; contract: _river.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _river_dress  # noqa: E402
import _river_fork_dress  # noqa: E402

PIECE = "river_fork_center"
root, lods, out, stats = _river_dress.build_dressing(PIECE, "fork", __file__, per_side=_river_fork_dress.PER_SIDE,
                                                     skip=_river_fork_dress.skip)
_river_dress.finish_dressing(root, lods, out, PIECE, __file__, stats)
