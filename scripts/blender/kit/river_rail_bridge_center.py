"""river_rail_bridge_center: the single-track rail (edges 1 and 4) across the straight river (edges 0
and 3) at 60 degrees on the road bridge's stone arch, on river_straight_base (P3, refs
assets-source/refs/rail and assets-source/refs/river_road_bridge). Scripted route (PRD section 11.1).
Geometry, edge contract and masks: _rail_bridge.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _rail_bridge  # noqa: E402

_rail_bridge.build_piece("river_rail_bridge_center", __file__)
