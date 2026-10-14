"""beach_outer_base: the bed of the headland tile beach_outer (the coast turns around grass edge 3). Grass, the beach
slope, the damp band, the shelf, the drop to the ocean bed and the soil skirt, in one piece (the base
is the bed, water contract §5). Scripted route. Profile, ring and slots: _beach.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _beach  # noqa: E402

_beach.build_base("beach_outer_base", "outer", __file__)
