"""river_road_bridge_center: the stone arch road bridge of river_road_bridge (road on edges 1 and 4
across the river on edges 0 and 3), on river_straight_base. Scripted route (PRD section 11.1).
Geometry and contract: _river_bridge.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _river_bridge  # noqa: E402

_river_bridge.build("river_road_bridge_center", __file__)
