"""river_mouth_l_center: dressing for the mirrored river mouth tile (River on edge 4), the mirror
image of river_mouth_center across the coast's seaward axis, built natively from reflected anchors
on the "l" bed (never a mirrored mesh). Kind `prop`, colour-only slots. Scripted route. Placement
and rules: _mouth_dress.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _mouth_dress  # noqa: E402

_mouth_dress.build("river_mouth_l_center", "l", __file__)
