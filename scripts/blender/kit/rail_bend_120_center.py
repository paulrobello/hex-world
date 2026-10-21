"""rail_bend_120_center: single track bending across edges two apart, edge-0 midpoint to edge-2
midpoint, on a 24 m arc centred where the two edge lines meet (P3, refs assets-source/refs/rail,
owner A17). Scripted route (PRD §11.1). Geometry: _rail_bend_wide.py; section, edge contract and
masks: _rail.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _rail_bend_wide  # noqa: E402

PIECE = "rail_bend_120_center"
# Arc segments per LOD (2.5 degrees at LOD0: 6 mm chord sag on the outer toe, as the 60-degree bend).
# 38 tris per segment plus 36 sleepers (10 tris at LOD0, 2 at LOD1, none at LOD2):
# 1272 / 528 / 152 tris (42%, 12%; ceilings 50% and 15%).
SEGMENTS = (24, 12, 4)

_rail_bend_wide.build_piece(PIECE, SEGMENTS, __file__)
