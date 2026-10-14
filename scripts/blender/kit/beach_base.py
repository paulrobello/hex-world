"""beach_base: the bed of the straight beach tile (edges 2 and 5 beach, ocean on 0-1, grass on 3-4). Grass, the beach
slope, the damp band, the shelf, the drop to the ocean bed and the soil skirt, in one piece (the base
is the bed, water contract §5). Scripted route. Profile, ring and slots: _beach.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _beach  # noqa: E402

_beach.build_base("beach_base", "straight", __file__)
