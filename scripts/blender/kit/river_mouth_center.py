"""river_mouth_center: dressing for the river mouth tile (refs/river_mouth, River on edge 3), which
sits on river_mouth_base (the bed): beach_center's dune grass, driftwood and pebbles on the two sand
shoulders, river_straight_center's bank stones and reeds along the channel. Kind `prop`,
colour-only slots. Scripted route. Placement and rules: _mouth_dress.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _mouth_dress  # noqa: E402

_mouth_dress.build("river_mouth_center", "r", __file__)
