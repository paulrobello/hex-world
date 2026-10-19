"""rail_straight_center: single straight track from the edge-3 midpoint to the edge-0 midpoint, on the
meadow base (P3, refs assets-source/refs/rail). Scripted route (PRD §11.1). Section, edge contract
and masks: _rail.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _rail  # noqa: E402

PIECE = "rail_straight_center"
# Path segments per LOD. 38 tris per segment (ballast 10, rails 28) plus 40 sleepers (10 tris each
# at LOD0, a 2-tri top at LOD1, none at LOD2): 704 / 156 / 38 tris (22%, 5%).
SEGMENTS = (8, 2, 1)

_rail.build_piece(PIECE, "straight", SEGMENTS, __file__)
