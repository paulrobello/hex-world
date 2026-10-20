"""rail_road_crossing_center: the single straight track (edges 0 and 3) crossed at 60 degrees by a two-lane
road (edges 1 and 4) on a level crossing with timber deck panels and four white field gates swung open
(P3, refs assets-source/refs/rail ref-3, owner A17). Scripted route (PRD §11.1). Edge contracts, deck,
ramps, gates and masks: _rail_crossing.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _rail_crossing  # noqa: E402

PIECE = "rail_road_crossing_center"
# Rail path segments and road-ramp rows per LOD (LOD0, LOD1, LOD2). LOD1 keeps two gate bars and no
# keep posts; LOD2 has no gates, no sleepers and a one-slab deck.
RAIL_SEGMENTS = (8, 2, 1)
ARM_ROWS = (8, 3, 1)

_rail_crossing.build_piece(PIECE, RAIL_SEGMENTS, ARM_ROWS, __file__)
