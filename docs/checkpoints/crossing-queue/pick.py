#!/usr/bin/env python3
"""Picks the scan dump with the longest queue at the closed crossing (see crossing_queue.py).

A car counts as queued when the crossing is closed, it is on the road phase, it stood still since
the dump before, and it is within 45 m of the crossing point on one approach. Prints the best dump
index, its queue (car id and distance to the crossing) and the centre-to-centre gaps.

Usage (repo root): python3 docs/checkpoints/crossing-queue/pick.py target/shots
"""

import json
import math
import pathlib
import sys

CROSSING = (-55.43, -96.0)


def load(d, i):
    p = d / f"cq-{i:03d}.json"
    if not p.exists():
        return None
    j = json.loads(p.read_text())
    return j.get("play", j)


def main():
    d = pathlib.Path(sys.argv[1])
    best = (0, None, [])
    prev = None
    i = 0
    while (cur := load(d, i)) is not None:
        if prev is not None and cur.get("closed_crossings"):
            was = {c["id"]: (c["x"], c["z"]) for c in prev["car_list"]}
            sides = {True: [], False: []}
            for c in cur["car_list"]:
                if c["phase"] != "road" or was.get(c["id"]) != (c["x"], c["z"]):
                    continue
                dist = math.hypot(c["x"] - CROSSING[0], c["z"] - CROSSING[1])
                if dist < 45:
                    sides[c["z"] > CROSSING[1]].append((round(dist, 2), c["id"], c["x"], c["z"]))
            for q in sides.values():
                if len(q) > best[0]:
                    best = (len(q), i, sorted(q))
        prev = cur
        i += 1
    n, at, q = best
    release = None
    if at is not None:
        k = at + 1
        while (cur := load(d, k)) is not None and cur.get("closed_crossings"):
            k += 1
        release = k + 3 if cur is not None else None
    print(f"scanned {i} dumps; longest queue {n} at dump {at}; release shot at dump {release}")
    for k, c in enumerate(q):
        gap = "" if k == 0 else f" gap {math.hypot(c[2] - q[k - 1][2], c[3] - q[k - 1][3]):.2f} m"
        print(f"  car {c[1]} {c[0]} m from the crossing{gap}")


if __name__ == "__main__":
    main()
