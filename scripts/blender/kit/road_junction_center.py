"""road_junction_center: the 3-way Y junction, lanes on edges 0, 2 and 4. Straight arms from the
joints to the mouth, an asphalt box with kerb fillets between the arms, arm dashes stopping at the
mouth and no paint in the box. Scripted route (PRD §11.1). Contract: _road_bend.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _road_bend  # noqa: E402

PIECE = "road_junction_center"
# Fillet segments per LOD. Arms (3 x 28), the box (24) and the paint (6) are fixed, 114 tris, so the
# fillets (11 tris per segment each) carry LOD0 above the LOD2 floor: 1038 / 444 / 147 (43%, 14%).
SEGMENTS = (28, 10, 1)

_road_bend.build_junction(PIECE, SEGMENTS, __file__)
