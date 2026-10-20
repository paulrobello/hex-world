#!/usr/bin/env python3
"""Writes docs/checkpoints/rail/rail-play.json, the trains-in-play evidence script (P3).

Two worlds, run natively with `--clock fixed` by play.sh:

1. The loop: six rail straights round the start meadow and three bends at its corners (a 216 m
   triangle, two trains lapping the same way round). A play-distance overview, then close-ups of a
   straight beside the start meadow, 1 s apart.
2. The Y (after `new_world`): a turnout at (0, -1) with its toe on the west arm (three straights to
   a line end), its straight leg east (three straights) and its branch north (a straight, the level
   crossing at (0, -3) and a straight), with a house cluster on each side of the crossing's road so
   cars drive over the deck. Two trains shuttle: each facing move at the toe takes the other leg. A
   play-distance overview, then sequences of shots, each with a state dump, at the junction, at the
   crossing and at eye level beside the west arm, from which play.sh keeps the frames named in it.

Usage: python3 docs/checkpoints/rail/rail_play.py (from the repo root).
"""

import json
from pathlib import Path

OUT = Path(__file__).with_name("rail-play.json")


def place(tile, cells):
    return [{"cmd": "select_tile", "id": tile}] + [{"cmd": "place", "q": q, "r": r} for q, r in cells]


def camera(pos, look_at):
    # A camera move changes the cell under a resting pointer, which hovers it and draws the ghost
    # there; hovering a far cell a moment later puts the ghost out of view, and it holds while the
    # camera and the pointer stay still.
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
    s = [
        # 0. A rail straight's ghost north-west of the start meadow at (0, -1), world (-13.86, -24),
        # 12 frames after selecting it and again with its meshes. Natively the village pack (the
        # rail pieces) has landed by the first shot, so play.sh keeps `rail-ghost` only; the lane
        # strokes drawn while the pack streams are the affordances test's.
        {"cmd": "camera", "pos": [-2, 34, 6], "look_at": [-12, 0, -22]},
        {"cmd": "select_tile", "id": "rail_straight"},
        {"cmd": "hover", "q": 0, "r": -1},
        {"cmd": "wait", "frames": 12},
        {"cmd": "screenshot", "name": "rail-ghost-strokes"},
        {"cmd": "dump_state", "name": "rail-ghost-strokes"},
        {"cmd": "wait", "frames": 240},
        {"cmd": "screenshot", "name": "rail-ghost"},
        {"cmd": "dump_state", "name": "rail-ghost"},
    ]
    # 1. The loop round the start meadow.
    s += place("rail_straight", [(0, -1), (1, -1), (1, 0), (0, 1), (-1, 1), (-1, 0)])
    s += place("rail_bend_60", [(2, -1), (-1, 2), (-1, -1)])
    s += [
        {"cmd": "hover", "q": 8, "r": -8},
        *camera([18, 62, 78], [0, 0, 0]),
        {"cmd": "wait", "frames": 900},
        {"cmd": "screenshot", "name": "rail-loop"},
        {"cmd": "dump_state", "name": "rail-loop"},
        # The straight at (1, -1), world (13.86, -24), from its south side.
        *camera([30, 6, -9], [12, 1.6, -24]),
    ]
    s += series("rail-loop-close", 12, 60)
    # 2. The Y with the level crossing on its branch.
    s += [{"cmd": "new_world"}, {"cmd": "wait", "frames": 30}]
    s += place("rail_straight", [(1, -1), (2, -1), (3, -1)])
    s += place("rail_junction", [(0, -1)])
    s += place("rail_straight", [(-1, -1), (-2, -1), (-3, -1), (0, -2)])
    s += place("rail_road_crossing", [(0, -3)])
    s += place("rail_straight", [(0, -4)])
    s += place("house_cluster", [(-1, -2), (1, -4)])
    s += [
        {"cmd": "hover", "q": 8, "r": -8},
        *camera([-6, 88, 40], [-14, 0, -52]),
        {"cmd": "wait", "frames": 900},
        {"cmd": "screenshot", "name": "rail-y"},
        {"cmd": "dump_state", "name": "rail-y"},
        # The turnout at (0, -1), world (-13.86, -24): both legs and the toe in view.
        *camera([2, 66, 34], [-6, 0, -32]),
    ]
    s += series("rail-junction", 40, 120)
    s += [
        # The level crossing at (0, -3), world (-41.57, -72), from the south-east.
        *camera([-21, 13, -58], [-41.6, 0.8, -72]),
    ]
    s += series("rail-crossing", 40, 90)
    s += [
        {"cmd": "walk", "on": True, "pointer_lock": None},
        # Beside the west arm, 7 m south of the track at z = -24, looking east along it toward the
        # junction, the way trains come from.
        {"cmd": "walk_to", "x": -62, "z": -16.5, "heading": 65, "pitch": -2},
        {"cmd": "wait", "frames": 30},
    ]
    s += series("rail-walk", 40, 45)
    s += [{"cmd": "walk", "on": False, "pointer_lock": None}, {"cmd": "exit"}]
    OUT.write_text("[\n" + ",\n".join("  " + json.dumps(c) for c in s) + "\n]\n")
    print(f"wrote {OUT} ({len(s)} commands)")


if __name__ == "__main__":
    main()
