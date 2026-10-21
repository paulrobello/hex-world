#!/usr/bin/env python3
"""Writes docs/checkpoints/rail_bend_120/play.json, the wide rail bend's in-play evidence script.

Six rail_bend_120 tiles on the six ring cells round the start meadow close one circle of track,
radius 24 m about the origin (150.8 m), which holds one train. Ring cell d is the origin's neighbour
across world edge d and needs rotation (d + 2) % 6 (lanes on world edges d + 2 and d + 4). Hovering a
slot auto-rotates the ghost to the first legal rotation (or the one last chosen with `rotate`, when
legal), so the cells whose first legal rotation is not the ring's get one `rotate` (cw) first:
cell 0 (legal 0, 2, 4, 5), cell 2 (legal 0, 4) and cell 3 (legal 1, 5). play.sh checks the placed
rotations in the dump.

Shots (each an offscreen `photo_export`, so a covered or sleeping window cannot blank them, plus a
state dump): the circle at play distance, then series 1 s apart from which play.sh keeps one frame
each (picked from the dumps' train position): the bend on ring cell 0 close from its sunny side, the
whole train on the curve about 22 m out, and eye level beside the track; then close-ups about 6 m
from the bend on its sun and shade sides. kit.json shoots the kit
scenario's wide bend where it meets the rail straight on (-3, 6).

Usage: python3 docs/checkpoints/rail_bend_120/play.py (from the repo root).
"""

import json
from pathlib import Path

OUT = Path(__file__).with_name("play.json")
RING = [(1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)]
ROTATE_FIRST = {0, 2, 3}


def camera(pos, look_at):
    return [
        {"cmd": "camera", "pos": pos, "look_at": look_at},
        {"cmd": "wait", "frames": 2},
        {"cmd": "hover", "q": 8, "r": -8},
    ]


def shot(name):
    return [
        {"cmd": "photo_export", "scale": 1, "name": name},
        {"cmd": "wait", "frames": 10},
        {"cmd": "dump_state", "name": name},
    ]


def main():
    s = [{"cmd": "select_tile", "id": "rail_bend_120"}]
    for d, (q, r) in enumerate(RING):
        s.append({"cmd": "hover", "q": q, "r": r})
        s.append({"cmd": "wait", "frames": 2})
        if d in ROTATE_FIRST:
            s.append({"cmd": "rotate", "dir": "cw"})
            s.append({"cmd": "wait", "frames": 2})
        s.append({"cmd": "place", "q": q, "r": r})
        s.append({"cmd": "wait", "frames": 2})
    s.append({"cmd": "dump_state", "name": "rb120-placed"})
    # The circle and the start meadow inside it, about 75 m out, from the sun's side.
    s += camera([16, 46, 56], [0, 0, 2])
    s += [{"cmd": "wait", "frames": 900}]
    s += shot("rb120-play")
    # The bend on ring cell 0 (1, 0): its arc's middle is world (24, 0); about 11 m out on the sunny
    # (+x, +z) side, a train passing every 25 s.
    s += camera([32, 5, 8], [24, 0.8, 0])
    for i in range(30):
        s += [{"cmd": "wait", "frames": 60}]
        s += shot(f"rb120-close-{i:02d}")
    # The whole train on the curve, about 22 m out from the sunny side of ring cell 0.
    s += camera([37, 10, 15], [18, 0.8, 7])
    for i in range(30):
        s += [{"cmd": "wait", "frames": 60}]
        s += shot(f"rb120-train-{i:02d}")
    # Eye level beside the track on ring cell 1 (0, 1), 6.9 m outside the circle, looking north along it
    # toward the bend on ring cell 0.
    s += [
        {"cmd": "walk", "on": True, "pointer_lock": None},
        {"cmd": "walk_to", "x": 22.6, "z": 21.0, "heading": 350, "pitch": -4},
        {"cmd": "wait", "frames": 30},
    ]
    for i in range(30):
        s += [{"cmd": "wait", "frames": 45}]
        s += shot(f"rb120-eye-{i:02d}")
    s += [{"cmd": "walk", "on": False, "pointer_lock": None}, {"cmd": "wait", "frames": 10}]
    # Close-ups about 6 m from the arc's middle on ring cell 0, world (24, 0): the sun side (+x, +z,
    # outside the circle) and the shade side (inside it, looking out at the inner ballast slope).
    s += camera([28.5, 2.2, 3.5], [24, 0.3, 0])
    s += [{"cmd": "wait", "frames": 10}]
    s += shot("rb120-sun")
    s += camera([19, 2.2, -2], [24, 0.3, 0])
    s += [{"cmd": "wait", "frames": 10}]
    s += shot("rb120-shade")
    s += [{"cmd": "exit"}]
    OUT.write_text("[\n" + ",\n".join("  " + json.dumps(c) for c in s) + "\n]\n")
    print(f"wrote {OUT} ({len(s)} commands)")


if __name__ == "__main__":
    main()
