"""river_mouth_base: the bed of the straight-beach river mouth with the River on edge 3 (beach edges 2
and 5, ocean 0-1, grass 4). The coast profile with the fanning channel carved in (the base is the
bed, water contract §3a, §5), one continuous cap, the soil skirt and the notch for the river_edge
stub. Scripted route. Profile: _mouth_profile.py; ring, rows, crease and refinement: _mouth.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _mouth  # noqa: E402

_mouth.build_base("river_mouth_base", "r", __file__)
