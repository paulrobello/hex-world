"""rail_tunnel_portal: the rail tunnel's arched portal, the road portal's rail counterpart (P4 rail at
levels; owner-approved references assets-source/refs/rail_tunnel_portal ref-2 for the look and ref-3 for
the layout, 2026-11-13, RL-Q1 O2): the road portal's grey granite with buff stones, lichen, moss and rain
streaks, its voussoirs, keystone, fern-hung coping and two amber wall lanterns, on a semicircular head
tall enough for the steam engine, under a turf mound that heaps over the wall and eases back down to the
cover level by edge 0. Scripted route; the rail dimensions, the mound and the train clearance are in
_tunnel_portal_rail.py over the approved road helper _tunnel_portal.py.

Built to the cutting (Ground::Cutting, as the road portal): the track stays at the tile's level and enters
the arch on edge 0. Opening 5.2 m wide, springing 2.4 m, crown 5.0 m over the tile (0.34 m over the
engine's chimney), wall top 5.85 m; face at runtime x 7.856, interior back plane at x 11.456; the sim's
`arch` node at [11.1, 0.0] (a ground hero at the origin of a straight-rail tile). Pack `village`, kind
`prop`, KTX2 class `piece`: one unique baked set (1024² albedo, 512² normal, ORM and masks, 512² PNG
fallback albedo)."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _tunnel_portal_rail

SPEC = {
    "piece": "rail_tunnel_portal",
    # Linear base colours and roughness of the colour-only slots, as in the manifest (the road portal's).
    "colors": {"stone": (0.30, 0.29, 0.27), "fern": (0.05, 0.12, 0.03), "turf": (0.10, 0.20, 0.05),
               "lining": (0.06, 0.058, 0.055), "back": (0.012, 0.012, 0.012), "iron": (0.035, 0.035, 0.038),
               "glass": (0.72, 0.33, 0.045),
               # Open tunnels (`_tunnel_portal_rail.OPEN_SLOTS`): each takes the colour of the slot it splits from.
               "lining_back": (0.06, 0.058, 0.055), "barrel_back": (0.30, 0.29, 0.27), "stone_back": (0.30, 0.29, 0.27),
               "stone_open": (0.30, 0.29, 0.27), "turf_open": (0.10, 0.20, 0.05)},
    "rough": {"stone": 0.9, "fern": 0.85, "turf": 0.9, "lining": 0.95, "back": 1.0, "iron": 0.55, "glass": 0.2,
              "lining_back": 0.95, "barrel_back": 0.9, "stone_back": 0.9, "stone_open": 0.9, "turf_open": 0.9},
}

_tunnel_portal_rail.build_piece(SPEC, __file__)
