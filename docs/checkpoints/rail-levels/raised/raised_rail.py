#!/usr/bin/env python3
"""Writes docs/checkpoints/rail-levels/raised/raised-rail.json, the raised-rail evidence script (P4).

The 216 m triangle loop of `docs/checkpoints/rail/rail_play.py` with every rail tile placed on level 1,
so two trains lap raised track round the level-0 start meadow: a play-distance overview, then a
close-up beside a straight and a low camera along the track, each with a state dump.

Usage: python3 docs/checkpoints/rail-levels/raised/raised_rail.py (from the repo root).
"""

import json
from pathlib import Path

OUT = Path(__file__).with_name("raised-rail.json")


def place(tile, cells, level):
    return [{"cmd": "select_tile", "id": tile}] + [
        {"cmd": "place", "q": q, "r": r, "level": level} for q, r in cells
    ]


def camera(pos, look_at):
    return [
        {"cmd": "camera", "pos": pos, "look_at": look_at},
        {"cmd": "wait", "frames": 2},
        {"cmd": "hover", "q": 8, "r": -8},
    ]


def series(name, n, every):
    out = []
    for i in range(n):
        out += [
            {"cmd": "wait", "frames": every},
            {"cmd": "screenshot", "name": f"{name}-{i:02d}"},
            {"cmd": "dump_state", "name": f"{name}-{i:02d}"},
        ]
    return out


def main():
    s = []
    s += place("rail_straight", [(0, -1), (1, -1), (1, 0), (0, 1), (-1, 1), (-1, 0)], 1)
    s += place("rail_bend_60", [(2, -1), (-1, 2), (-1, -1)], 1)
    s += [
        {"cmd": "hover", "q": 8, "r": -8},
        *camera([18, 70, 78], [0, 4, 0]),
        {"cmd": "wait", "frames": 900},
        {"cmd": "screenshot", "name": "raised-rail-overview"},
        {"cmd": "dump_state", "name": "raised-rail-overview"},
        # The straight at (1, -1), world (13.86, -24), from its south side, a level up (4 m).
        *camera([30, 10, -9], [12, 5.6, -24]),
    ]
    s += series("raised-rail-close", 8, 60)
    s += [
        # Along the track from the ground beside the raised loop.
        *camera([40, 2.5, 0], [12, 5.0, -24]),
    ]
    s += series("raised-rail-low", 8, 60)
    s += [{"cmd": "exit"}]
    OUT.write_text("[\n" + ",\n".join("  " + json.dumps(c) for c in s) + "\n]\n")
    print(f"wrote {OUT} ({len(s)} commands)")


if __name__ == "__main__":
    main()
