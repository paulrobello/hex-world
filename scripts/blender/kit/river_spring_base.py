"""river_spring_base: the lane-cut base for the river_spring tile (River on edge 0 only, the stream
running from edge 0 to the spring pool at N = (-2, 0)). meadow_base with the river_straight channel
carved along the stream and a round pool bowl about the node: one continuous cap + bank + bed
surface, the soil skirt everywhere but the edge-0 channel span, the notch for the river_edge stub at
edge 0. Scripted route (PRD section 11.1). Pinned bed and mesh: _spring.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _spring  # noqa: E402

PIECE = "river_spring_base"
# Per LOD: pool rings about the node as (radius, spokes), on the bowl's kinks 4.165 and 5.95 and
# spaced so the radial interpolation and the chord sag (slope x r x dtheta^2 / 8) stay inside the
# contract; stream row spacing near the node; stream rows from the crease end to the joint; meadow
# lattice spacing. The ring, the joint row and the profile stay fixed; refinement closes the rest.
POOL = [(1.05, 8), (2.1, 11), (3.15, 16), (4.165, 21), (4.515, 28), (4.965, 33), (5.45, 30), (5.95, 20),
        (7.6, 15), (8.45, 10)]
LOD2 = {"rings": [(1.05, 7), (2.1, 11), (3.15, 15), (4.165, 21), (4.45, 25), (4.74, 29), (5.05, 31), (5.45, 30),
                  (5.95, 25), (7.6, 15), (8.45, 10)], "row_dx": lambda t: 1.0, "stream_rows": 1, "spacing": None,
        "columns": True}
LOD1 = {"rings": [(r, int(n * 1.5)) for r, n in POOL] + [(0.55, 6), (1.6, 12), (2.6, 16), (3.7, 22), (6.7, 26)],
        "row_dx": lambda t: 0.7, "stream_rows": 4, "spacing": 1.8}
LOD0 = {"rings": [(r, int(n * 2.2)) for r, n in POOL] + [(0.55, 10), (1.6, 18), (2.6, 24), (3.7, 32), (4.74, 66),
                                                        (5.2, 66), (5.7, 50), (6.4, 40), (7.0, 36), (8.0, 30)],
        "row_dx": lambda t: 0.45, "stream_rows": 10, "spacing": 0.76}
SKIRT_ROWS = (4, 2, 1)

_spring.build_spring_base(PIECE, (LOD0, LOD1, LOD2), SKIRT_ROWS, __file__)
