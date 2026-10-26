"""road_bend_60_edge_e1: road_straight_edge trimmed at road_bend_60's inner bisector, for the tile's
lane 1 (in its own edge-0 frame the inner side is +y): road_bend_60_edge_e0 mirrored. Scripted
route (PRD §11.1). Profile: _road.py; trim and joint contract: _road_bend_trim.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _road_bend_trim  # noqa: E402

PIECE = "road_bend_60_edge_e1"
# Length segments per LOD, as road_bend_60_edge_e0.
SEGMENTS = (9, 3, 1)

_road_bend_trim.build_stub(PIECE, 1, SEGMENTS, __file__)
