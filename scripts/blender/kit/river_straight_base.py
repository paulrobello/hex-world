"""river_straight_base: the channel-cut base for river lanes on edges 0 and 3 (river_straight and
river_road_bridge). meadow_base with the straight channel carved in: one continuous cap + bank + bed
surface, the soil skirt everywhere but the channel spans, a notch for the river_edge stub at each
lane edge. Scripted route (PRD section 11.1). Profile and contract: _river.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _river  # noqa: E402

PIECE = "river_straight_base"
# Channel rows, meadow lattice spacing and skirt rows per LOD (the ring and the profile stay fixed).
ROWS = (32, 14, 3)
SPACING = (0.9, 1.8, None)
SKIRT_ROWS = (4, 2, 1)

_river.build_base(PIECE, "straight", ROWS, SPACING, SKIRT_ROWS, __file__)
