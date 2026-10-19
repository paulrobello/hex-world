"""rail_bend_60_center: single track bending across adjacent edges, edge-0 midpoint to edge-1
midpoint, on an 8 m arc centred on corner 0 (P3, refs assets-source/refs/rail, owner A17). Scripted
route (PRD §11.1). Section, edge contract and masks: _rail.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _rail  # noqa: E402

PIECE = "rail_bend_60_center"
# Arc segments per LOD (3.75 degrees at LOD0: 6 mm chord sag on the outer toe). 38 tris per segment
# plus 24 sleepers (10 tris at LOD0, 2 at LOD1, none at LOD2): 1456 / 504 / 152 tris (35%, 10%).
SEGMENTS = (32, 12, 4)

_rail.build_piece(PIECE, "bend", SEGMENTS, __file__)
