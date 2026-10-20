"""rail_station_shelter: the ref-1 station shelter (owner A17; refs assets-source/refs/rail_station), a
prop standing on rail_station_platform's paving (pivot at its footprint centre, y = platform top).
Scripted route; geometry and the bake are in _station.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _station  # noqa: E402

_station.build_piece("shelter", __file__)
