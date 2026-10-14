"""road_straight_center: straight road through the hex centre, x in [-6, 6]. Scripted route
(PRD §11.1). Profile, dash phase and contract: _road.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))
from lib import export  # noqa: E402

import _road  # noqa: E402

PIECE = "road_straight_center"
# LODs change only the length segments (C2). S = 28 tris per segment, p = 6 paint tris: 342 / 146 / 34.
SEGMENTS = (12, 5, 1)
# Verge sign posts on the flat verge top (|t| in [3.25, 4.0], z = VERGE_TOP), midway between the
# kerb and the slope, so a raised meadow cap cannot bury them (Task 9 review M2).
POST_T = (_road.ROAD_WIDTH / 2 + _road.KERB_W + _road.ROAD_WIDTH / 2 + _road.VERGE) / 2


def sockets(root):
    for n, (x, t) in enumerate(((-3.0, POST_T), (3.0, -POST_T))):
        export.add_socket(f"anchor-prop-{n}", (x, t, _road.VERGE_TOP), root, kind="prop", radius=0.5)


_road.build_piece(PIECE, -_road.CENTER_HALF, _road.CENTER_HALF, SEGMENTS, __file__, sockets)
