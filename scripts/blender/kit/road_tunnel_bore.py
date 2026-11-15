"""road_tunnel_bore: one cover cell of an open road tunnel (P4 open tunnels, owner 2026-11-14), edge to edge
on the tunnel axis, the road portal's section and stone, the road on its floor and six always-lit wall lamps.
Everything is in _road_tunnel_bore.py."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _road_tunnel_bore  # noqa: E402

_road_tunnel_bore.build_piece(__file__)
