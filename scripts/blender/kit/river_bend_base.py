"""river_bend_base: the channel-cut base for river lanes on edges 0 and 2 (river_bend). meadow_base
with the 24 m bend arc carved in: one continuous cap + bank + bed surface, the soil skirt everywhere
but the channel spans, a notch for the river_edge stub at each lane edge. Scripted route
(PRD section 11.1). Profile and contract: _river.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _river  # noqa: E402
import _river_flat  # noqa: E402,F401  (the flattened meadow relief, owner D6-Q1)

PIECE = "river_bend_base"
# Radial rows between the joints, meadow lattice spacing and skirt rows per LOD (ring and seam rows
# fixed). Every LOD stays inside the water contract (`_river.profile_errors`): the chord sag of
# the arc needs 16 rows even at LOD2, so LOD2 thins the interior rows to eight offsets per side
# (the bed parabola, both kinks, a mid-bank and the bank top) and uses the lane-cut LOD2 ceiling.
ROWS = (33, 18, 16)
SPACING = (0.85, 1.8, None)
SKIRT_ROWS = (4, 2, 1)
LOD2_HALF = (0.0, 0.8, 2.0, _river.KINKS[0], 3.5, _river.KINKS[1], 5.7, _river.BANK_TOP)
INNER = (None, None, _river.symmetric(LOD2_HALF))

_river.build_base(PIECE, "bend", ROWS, SPACING, SKIRT_ROWS, __file__, INNER)
