"""rail_station_platform: the P3 side platform (owner A17; refs assets-source/refs/rail_station): 20 m
level brick platform with ramps, coping, lamps, nameboard and flower tubs, placed with
rail_straight_center. Scripted route; geometry, clearances and the bake are in _station.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _station  # noqa: E402

_station.build_piece("platform", __file__)
