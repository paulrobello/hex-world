"""forest_forest_edge_e3: the forest floor over forest_base's rim for a Forest/Forest edge at
forest_base's local edge 3. Scripted route (PRD §11.1). Contract and builder: _forest_edge.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _forest_edge  # noqa: E402

_forest_edge.build("forest_forest_edge_e3", 3, __file__)
