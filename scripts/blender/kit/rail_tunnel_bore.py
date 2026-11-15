"""rail_tunnel_bore: the stone-lined bore a complete rail tunnel draws under each cover cell (open
tunnels, owner 2026-11-14; section ruling O1, owner 2026-11-14: a turf ridge over the bore). The rail
portal's open interior carried on from edge 0 to edge 0 under a turf ridge, with the track and four
always-lit amber lamps. Geometry, ring, ridge, lamps and track in _rail_tunnel_bore.py over the rail
portal's open-tunnel contract (_tunnel_portal_rail.py) and `_rail.track`.

Pack `village`, kind `prop`, KTX2 class `piece`: the rail portal's baked set (1024² albedo, 512² normal,
ORM and masks, 512² PNG fallback albedo) with the bore's parts on its unused islands, and a 256² emissive
map for the lamps."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _rail
import _rail_tunnel_bore

SPEC = {
    "piece": "rail_tunnel_bore",
    # Linear base colours and roughness (the rail portal's stone, iron, glass and turf; the shared rail slots').
    "colors": {"stone": (0.30, 0.29, 0.27), "glass": (0.72, 0.33, 0.045), "iron": (0.035, 0.035, 0.038),
               "turf": (0.10, 0.20, 0.05),
               _rail.BALLAST: (0.205, 0.202, 0.21), _rail.SLEEPER: _rail.SLEEPER_COLOR, _rail.STEEL: _rail.STEEL_COLOR,
               _rail.HEAD: _rail.HEAD_COLOR},
    "rough": {"stone": 0.9, "glass": 0.2, "iron": 0.55, "turf": 0.9, _rail.BALLAST: 0.95, _rail.SLEEPER: _rail.SLEEPER_ROUGH,
              _rail.STEEL: _rail.STEEL_ROUGH, _rail.HEAD: _rail.HEAD_ROUGH},
}

_rail_tunnel_bore.build_piece(SPEC, __file__)
