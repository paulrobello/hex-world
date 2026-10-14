"""road_bend_60_center: the sharp bend, edge-0 lane to edge-1 lane, an arc of radius 6 tan 30 deg
(3.464 m) tangent to both arms at the joints, with the inner-side fill described in _road_bend.py.
Scripted route (PRD §11.1). Profile: _road.py; bend, joint and dash contract: _road_bend.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _road_bend  # noqa: E402

PIECE = "road_bend_60_center"
# Arc segments per LOD. S = 28 tris per segment (24 swept, 4 inner fill) plus the paint:
# 700 / 320 / 88 tris (46%, 13%).
SEGMENTS = (24, 11, 3)

_road_bend.build_bend(PIECE, 1, SEGMENTS, __file__)
