"""Writes the share-image play script (owner R8, 2026-10-14: "framed like the owner's play screenshot":
a house cluster with its road, a river bending round the oak from a spring pool, pine forest behind,
meadows and a beach in front). Every placement is forced to one rotation by the tiles placed before
it, so the world is the same on every run. Usage: python3 share-play.py <out.json>; run with
`--scenario play --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed`.
Each framing is captured in photo mode (free-slot outlines fade out) as share-<name>.png."""

import json
import sys

R = 16.0
SQ3 = 3**0.5


def centre(q: int, r: int) -> list[float]:
    return [R * SQ3 * (q + r / 2), R * 1.5 * r]


TILES = [
    # Pine forest behind (north), forcing the spring's and the road's rotations.
    ("forest", 0, -1), ("forest", 1, -1), ("forest", 2, -1), ("forest", 0, -2), ("forest", 1, -2),
    ("forest", 2, -2), ("forest", 3, -2), ("forest", 3, -1),
    # The road the cluster sits on runs north into the trees.
    ("road_straight", -1, 0), ("road_straight", -1, -1), ("forest", -2, -1), ("forest", -2, 0),
    # Meadows east, the oak inside the bend, the coast in front.
    ("meadow", 2, 0), ("meadow", 2, 1), ("meadow", 3, 0), ("meadow", 3, 1), ("oak", 1, 1),
    ("beach", 1, 2), ("beach", 2, 2), ("beach", 3, 2),
    ("river_mouth", 0, 2), ("river_mouth_l", 0, 2),
    ("river_spring", 1, 0), ("river_bend", 0, 1),
    ("beach", -1, 2), ("house_cluster", -1, 1), ("meadow", -2, 1), ("beach", -2, 2),
    ("ocean", -3, 3), ("ocean", -2, 3), ("ocean", -1, 3), ("ocean", 0, 3), ("ocean", 1, 3),
    ("ocean", 2, 3), ("ocean", 3, 3), ("ocean", -3, 4), ("ocean", -2, 4), ("ocean", -1, 4),
    ("ocean", 0, 4), ("ocean", 1, 4), ("ocean", 2, 4),
]

# (name, camera position, look-at), world metres.
FRAMINGS = [
    ("a-front-high", [20.0, 62.0, 112.0], [14.0, 0.0, 10.0]),
    ("b-front-left", [-48.0, 42.0, 96.0], [20.0, 0.0, 6.0]),
    ("c-front-right", [86.0, 40.0, 92.0], [4.0, 0.0, 4.0]),
    ("d-front-right-wide", [96.0, 48.0, 108.0], [-8.0, 0.0, 10.0]),
]

out: list[dict] = [{"cmd": "camera", "pos": [14, 95, 125], "look_at": [14, 0, 16]}, {"cmd": "wait", "frames": 30}]
for tile, q, r in TILES:
    out += [{"cmd": "select_tile", "id": tile}, {"cmd": "hover", "q": q, "r": r}, {"cmd": "place", "q": q, "r": r}]
out += [
    {"cmd": "hover", "q": 9, "r": -9},
    {"cmd": "wait", "frames": 600},
    {"cmd": "dump_state", "name": "share-play-state"},
]
for name, pos, look in FRAMINGS:
    out += [
        {"cmd": "camera", "pos": pos, "look_at": look},
        {"cmd": "wait", "frames": 10},
        {"cmd": "toggle", "what": "photo"},
        {"cmd": "photo", "dof": "off", "pause_agents": False},
        {"cmd": "wait", "frames": 60},
        {"cmd": "screenshot", "name": f"share-{name}"},
        {"cmd": "toggle", "what": "photo"},
        {"cmd": "wait", "frames": 20},
    ]
out.append({"cmd": "exit"})
with open(sys.argv[1], "w") as f:
    json.dump(out, f, indent=1)
    f.write("\n")
print(f"wrote {sys.argv[1]}: {len(out)} commands; tile centres e.g. oak {centre(1, 1)}")
