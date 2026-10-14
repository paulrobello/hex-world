"""road_straight_edge: straight road from the centre piece's end (x = 6) to the edge-0 border
(x = APOTHEM). Scripted route (PRD §11.1). Profile, dash phase and contract: _road.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))
from lib import export, hexgeo  # noqa: E402

import _road  # noqa: E402

PIECE = "road_straight_edge"
# LODs change only the length segments (C2). Paint is identical in every LOD, so the ratios are
# (S*n + p) / (S*n0 + p) with S = 28 tris per segment and p = 4 paint tris: 256 / 116 / 32.
SEGMENTS = (9, 4, 1)


def sockets(root):
    c = hexgeo.lane_frame(0, 0.0, _road.ROAD_WIDTH)["center"]
    export.add_socket("socket-lane-0", (*c, 0.0), root, kind="road", edge=0, offset=0.0,
                      width=_road.ROAD_WIDTH)


_road.build_piece(PIECE, _road.CENTER_HALF, hexgeo.APOTHEM, SEGMENTS, __file__, sockets)
