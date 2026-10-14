"""road_bend_120_center: the wide bend, edge-0 lane to edge-2 lane, an arc of radius 6 tan 60 deg
(10.392 m) tangent to both arms at the joints. Scripted route (PRD §11.1). Profile: _road.py;
bend, joint and dash contract: _road_bend.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _road_bend  # noqa: E402

PIECE = "road_bend_120_center"
# Arc segments per LOD (C2: LODs change only the length segments). S = 28 tris per segment plus
# the paint: 468 / 208 / 62 tris (44%, 13%).
SEGMENTS = (16, 7, 2)

_road_bend.build_bend(PIECE, 2, SEGMENTS, __file__)
