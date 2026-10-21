"""river_fork_base: the channel-cut base for the river_fork tile (river lanes on edges 0, 2 and 4 meeting
at the centre). meadow_base with the Y carved in: one continuous cap + bank + bed surface, the soil
skirt everywhere but the three channel spans, a notch for the river_edge stub at each lane edge.
Scripted route (PRD section 11.1). Bed and layout: _river_fork.py; profile and contract: _river.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _river  # noqa: E402
import _river_flat  # noqa: E402,F401  (the flattened meadow relief, owner D6-Q1)
import _river_fork  # noqa: E402,F401  (registers the "fork" layout)

PIECE = "river_fork_base"
# Rows per wedge between the crease and the joint, meadow lattice spacing and skirt rows per LOD (the
# ring, the joint rows and the crease stay fixed). The bed is exact along every line of constant
# offset, so the coarse LODs keep the profile with a single row step.
ROWS = (14, 5, 1)
SPACING = (0.85, 1.8, None)
SKIRT_ROWS = (4, 2, 1)

_river.build_base(PIECE, "fork", ROWS, SPACING, SKIRT_ROWS, __file__)
