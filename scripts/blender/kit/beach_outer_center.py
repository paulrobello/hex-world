"""beach_outer_center: dressing for the headland tile beach_outer (refs/beach ref-1), which sits on beach_outer_base
(the bed): dune-grass tufts along the grass edge, driftwood on the dry sand, pebbles at the grass
edge. Kind `prop`, colour-only slots. Scripted route. Placement and rules: _beach_dress.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _beach_dress  # noqa: E402

_beach_dress.build_centre("beach_outer_center", "outer", __file__)
