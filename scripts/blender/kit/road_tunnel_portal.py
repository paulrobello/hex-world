"""road_tunnel_portal: the road tunnel's arched portal (P4 elevation; owner-approved reference
assets-source/refs/road_tunnel_portal ref-2, 2026-10-29): a head wall of weathered grey granite with a few
buff stones in pale lime mortar, lichen, moss in the joints and rain streaks; an elliptical arch of dressed
granite voussoirs with a projecting keystone on dressed jambs; a two-course coping with a drip edge, turf
and ferns over it; a stone-lined recess fading to black; two amber wall lanterns on scrolled brackets.
Scripted route; the high-poly templates, the shared-island bake, the LODs and the ground field are in
_tunnel_portal.py.

Built to the cut row (controller Q1): the road stays at the tile's level through a cutting and enters the
arch on edge 0. Opening 7.8 m wide, springing 0.6 m, crown 3.15 m over the road's ground (at most 3.6 m),
wall top 4.0 m flush with the cover meadow; face at runtime x 11.656, interior back plane at x 13.656; the
sim's `arch` node at [13.3, 0.0] (wiring W1: a ground hero at the origin of a straight-road tile). Pack
`core`, kind `prop`, KTX2 class `piece`: one unique baked set (1024² albedo, 512² normal, ORM and masks)."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _tunnel_portal  # noqa: E402

SPEC = {
    "piece": "road_tunnel_portal",
    # Linear base colours and roughness of the colour-only slots, as in the manifest. The stone and the
    # ferns sample the baked set; the turf the world-space grass set (its colour here is for stills).
    "colors": {"stone": (0.30, 0.29, 0.27), "fern": (0.05, 0.12, 0.03), "turf": (0.10, 0.20, 0.05),
               "lining": (0.06, 0.058, 0.055), "back": (0.012, 0.012, 0.012), "iron": (0.035, 0.035, 0.038),
               "glass": (0.72, 0.33, 0.045)},
    "rough": {"stone": 0.9, "fern": 0.85, "turf": 0.9, "lining": 0.95, "back": 1.0, "iron": 0.55, "glass": 0.2},
}

_tunnel_portal.build_piece(SPEC, __file__)
