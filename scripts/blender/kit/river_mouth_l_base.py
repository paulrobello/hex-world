"""river_mouth_l_base: the mirrored river mouth bed, River on edge 4 (beach edges 2 and 5, ocean 0-1,
grass 3). Generated natively: river_mouth_base's 2D inputs reflected across the coast's seaward axis
before triangulating, heights from _mouth_profile hand "l" (never a negative scale). Scripted route.
Profile: _mouth_profile.py; construction: _mouth.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _mouth  # noqa: E402

_mouth.build_base("river_mouth_l_base", "l", __file__)
