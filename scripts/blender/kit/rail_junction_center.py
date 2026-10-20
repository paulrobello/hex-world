"""rail_junction_center: a single-track turnout. The straight runs edge 0 to edge 3 as
`rail_straight_center`; the branch leaves at the switch by edge 0 on `rail_bend_60_center`'s 8 m arc
about corner 0 and exits at edge 1 (P3, refs assets-source/refs/rail, owner A17). Scripted route
(PRD §11.1). Switch, frog, timbers, ground frame and masks: _rail_junction.py; section: _rail.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _rail_junction  # noqa: E402

PIECE = "rail_junction_center"

_rail_junction.build_piece(PIECE, __file__)
