#!/usr/bin/env python3
"""Writes the crossing-queue evidence scripts (owner report 2026-10-21: cars queue at a closed
level crossing instead of stacking on one spot).

The world is `crossing-queue.hexworld` (seed 1): the level crossing at (0, -4), world (-55.4, -96),
on a five-tile line with a shuttling train; south of it a road straight and a junction to two house
clusters, north of it a junction to two more. Seeded departures send cars across, and the closed
crossing holds them.

`scan` loads the world and dumps the state every 30 frames (0.5 s at `--clock fixed`) for 210 s
of game time, so the frame with the longest queue can be found from the dumps without shooting
every frame. `shot --at N --release R` replays the same world and shoots the queue at dump N and
again at dump R, as the queue pulls away; pick.py prints both from the scan's dumps. The scan and the
shot are not guaranteed to line up: on 2026-10-25 the scan (800x500, host load near 790) found the
queue at dump 158, while two 1600x1000 shot runs (load 500 to 550) agreed with each other and
reached it at dump 172 (`shot.json`: `--at 172 --release 186`). Scan at the shot's size and
confirm the frame from the shot run's own dumps.

Usage (repo root):
  python3 docs/checkpoints/crossing-queue/crossing_queue.py scan > target/cq-scan.json
  python3 docs/checkpoints/crossing-queue/crossing_queue.py shot --at N --release R \
    > docs/checkpoints/crossing-queue/shot.json
"""

import argparse
import json

SAVE = "docs/checkpoints/crossing-queue/crossing-queue.hexworld"
EVERY = 30


def head():
    return [
        {"cmd": "load", "path": SAVE},
        {"cmd": "wait", "frames": 30},
        # The crossing's south approach from the south-east, the deck at the top of the frame.
        {"cmd": "camera", "pos": [-28, 16, -66], "look_at": [-50, 0.5, -90]},
        {"cmd": "wait", "frames": 2},
        {"cmd": "hover", "q": 8, "r": -8},
    ]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["scan", "shot"])
    ap.add_argument("--at", type=int, default=0)
    ap.add_argument("--release", type=int, default=0)
    ap.add_argument("--count", type=int, default=420)
    a = ap.parse_args()
    s = head()
    if a.mode == "scan":
        for i in range(a.count):
            s += [{"cmd": "wait", "frames": EVERY}, {"cmd": "dump_state", "name": f"cq-{i:03d}"}]
    else:
        s += [{"cmd": "wait", "frames": EVERY * (a.at + 1)}]
        s += [{"cmd": "screenshot", "name": "crossing-queue"}, {"cmd": "dump_state", "name": "crossing-queue"}]
        if a.release > a.at:
            s += [
                {"cmd": "wait", "frames": EVERY * (a.release - a.at)},
                {"cmd": "screenshot", "name": "crossing-queue-release"},
                {"cmd": "dump_state", "name": "crossing-queue-release"},
            ]
    s += [{"cmd": "exit"}]
    print("[\n" + ",\n".join("  " + json.dumps(c) for c in s) + "\n]")


if __name__ == "__main__":
    main()
