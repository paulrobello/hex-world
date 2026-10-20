#!/usr/bin/env python3
"""Writes docs/checkpoints/rail-routing/play.json, the train-routing evidence script (P3).

The world: a turnout at (0, -1), world (-13.86, -24), with its toe on the west arm (two straights
to a line end), its straight leg east to a station at (1, -1) and its branch north-west to a station
at (0, -2). 156 m of track gets one train. It takes the two stations in turn as destinations: from
either station it runs out over the toe to the west line end, turns round and takes the leg its
route needs to the other.

A play-distance overview, then a series of shots 1.5 s apart, each with a state dump, from a camera
that holds both stations and the junction; play.sh keeps the first frame with the train standing at
each station with that station as its destination.

Usage: python3 docs/checkpoints/rail-routing/rail_routing.py (from the repo root).
"""

import json
from pathlib import Path

OUT = Path(__file__).with_name("play.json")
STATIONS = {"east": (1, -1), "north": (0, -2)}


def place(tile, cells):
    return [{"cmd": "select_tile", "id": tile}] + [{"cmd": "place", "q": q, "r": r} for q, r in cells]


def camera(pos, look_at):
    # A camera move changes the cell under a resting pointer, which hovers it and draws the ghost
    # there; hovering a far cell a moment later puts the ghost out of view.
    return [
        {"cmd": "camera", "pos": pos, "look_at": look_at},
        {"cmd": "wait", "frames": 2},
        {"cmd": "hover", "q": 8, "r": -8},
    ]


def main():
    # Auto-rotation lays the turnout toe west (edge 3), straight leg east and branch north-west
    # (edge 4), the only way that keeps rail off the start meadow and meets the east station
    # (asserted by the rail_routing_checkpoint test in crates/hex-app/src/sim.rs).
    s = place("rail_station", [STATIONS["east"]])
    s += place("rail_junction", [(0, -1)])
    s += place("rail_straight", [(-1, -1), (-2, -1)])
    s += place("rail_station", [STATIONS["north"]])
    s += [
        {"cmd": "hover", "q": 8, "r": -8},
        *camera([-12, 78, 28], [-16, 0, -36]),
        {"cmd": "wait", "frames": 600},
        {"cmd": "screenshot", "name": "routing-overview"},
        {"cmd": "dump_state", "name": "routing-overview"},
        # Both stations and the turnout: east (13.86, -24), north (-27.7, -48).
        *camera([6, 50, 16], [0, 0, -34]),
    ]
    for i in range(110):
        s += [
            {"cmd": "wait", "frames": 90},
            {"cmd": "screenshot", "name": f"routing-{i:03d}"},
            {"cmd": "dump_state", "name": f"routing-{i:03d}"},
        ]
    s += [{"cmd": "exit"}]
    OUT.write_text("[\n" + ",\n".join("  " + json.dumps(c) for c in s) + "\n]\n")
    print(f"wrote {OUT} ({len(s)} commands)")


if __name__ == "__main__":
    main()
