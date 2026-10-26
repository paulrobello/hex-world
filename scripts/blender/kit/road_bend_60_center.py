"""road_bend_60_center: the sharp bend, edge-0 lane to edge-1 lane, an arc of radius 6 tan 30 deg
(3.464 m) tangent to both arms at the joints. Its inner side ends at the pit fan inside the inner
kerb, apex at the arc centre, and meets the tile's trimmed stubs road_bend_60_edge_e0/e1 there
(_road_bend_trim.py). Scripted route (PRD §11.1). Profile: _road.py; bend, joint and dash contract:
_road_bend.py; the trimmed inner corner: _road_bend_trim.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _road_bend_trim  # noqa: E402

PIECE = "road_bend_60_center"
# Arc segments per LOD. S = 25 tris per segment (24 swept, 1 pit fan) plus the paint:
# 628 / 287 / 79 tris (46%, 13%).
SEGMENTS = (24, 11, 3)

_road_bend_trim.build_center(PIECE, SEGMENTS, __file__)
