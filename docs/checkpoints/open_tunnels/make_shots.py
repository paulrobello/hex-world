#!/usr/bin/env python3
"""Writes the open-tunnel review scripts (P4, owner 2026-11-14): `rail-tunnel-shots.json` and
`road-tunnel-shots.json`, run by `make shots-open-tunnels` under a booked load-sensitive window.

Rail: a column of rail tiles down q = 1 (world edges 1 and 4, the way the sim tests lay it), two
level-1 cover meadows between a portal at (1, 2) and one at (1, 5). Road: the elevation smoke
world (`docs/checkpoints/elevation/smoke.json`), whose portals at (2, 2) and (4, 2) hold one cover
cell at (3, 2). Each script shoots the overview, the mouth from outside, the ridge from the side,
the bore from inside with a vehicle passing, a walk-mode view inside the bore and the lamps.

Usage: python3 docs/checkpoints/open_tunnels/make_shots.py (from the repo root).
"""

import json
import math
from pathlib import Path

HERE = Path(__file__).parent
SPACING = 27.7128


def world(q, r):
    """A cell's centre in world metres (x, z)."""
    return SPACING * (q + r / 2), 24.0 * r


def cam(pos, look):
    return [
        {"cmd": "camera", "pos": pos, "look_at": look},
        {"cmd": "wait", "frames": 3},
        {"cmd": "hover", "q": 12, "r": -12},
    ]


def shot(name, n=1, every=1):
    out = []
    for i in range(n):
        tag = name if n == 1 else f"{name}-{i:02d}"
        out += [
            {"cmd": "wait", "frames": every},
            {"cmd": "screenshot", "name": tag},
            {"cmd": "dump_state", "name": tag},
        ]
    return out


def place(tile, q, r, level=None):
    c = {"cmd": "place", "q": q, "r": r}
    if level is not None:
        c["level"] = level
    return [{"cmd": "select_tile", "id": tile}, c]


def along(a, b, t):
    return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]


def views(a, b, d, out, forward_first=False):
    """Shots for a tunnel from portal cell centre `a` to `b` (x, z) along unit vector `d`."""
    mid = along(a, b, 0.5)
    # Walk headings run clockwise from -Z (docs/agent-operability.md `walk_to`).
    yaw_deg = math.degrees(math.atan2(d[0], -d[1]))
    left = [-d[1], d[0]]
    s = []
    # Inside the bore first, while the line's vehicles are still crossing it. The camera near the
    # far portal looks back along the bore: the train leaves in the first seconds (its rear), turns
    # at the line's end and returns head-on later (its front, headlights lit). The forward view of
    # the empty bore and its lamps comes last.
    back = cam([mid[0] + d[0] * 8, 1.7, mid[1] + d[1] * 8], [mid[0] - d[0] * 24, 1.9, mid[1] - d[1] * 24])
    fwd = cam([mid[0] - d[0] * 8, 1.7, mid[1] - d[1] * 8], [mid[0] + d[0] * 24, 1.9, mid[1] + d[1] * 24])
    if forward_first:
        # The road: cars drive toward the first portal in the first seconds, so the forward view
        # meets them head-on with their headlights lit.
        s += fwd + shot(f"{out}-inside", 14, 45)
        s += back + shot(f"{out}-inside-back", 4, 60)
    else:
        s += back + shot(f"{out}-inside-back", 36, 45)
        s += fwd + shot(f"{out}-inside", 4, 30)
    s += cam([mid[0] + 60, 70, mid[1] + 60], [mid[0], 0, mid[1]])
    s += shot(f"{out}-overview")
    # The mouth of the first portal from the cutting side, 18 m back at eye height.
    s += cam([a[0] - d[0] * 18, 2.6, a[1] - d[1] * 18], [a[0] + d[0] * 9, 1.6, a[1] + d[1] * 9])
    s += shot(f"{out}-mouth")
    # The ridge and the cover cells from the side, low.
    s += cam([mid[0] + left[0] * 34, 9, mid[1] + left[1] * 34], [mid[0], 4.5, mid[1]])
    s += shot(f"{out}-side")
    # Walk mode inside the bore.
    s += [
        {"cmd": "walk", "on": True, "pointer_lock": None},
        {"cmd": "walk_to", "x": mid[0] - d[0] * 6, "z": mid[1] - d[1] * 6, "heading": yaw_deg, "pitch": -2},
        {"cmd": "wait", "frames": 30},
    ]
    s += shot(f"{out}-walk", 3, 60)
    s += [{"cmd": "walk", "on": False, "pointer_lock": None}]
    return s


def rail():
    s = []
    s += place("rail_straight", 1, 0) + place("rail_straight", 1, 1)
    s += place("rail_tunnel_portal", 1, 2)
    s += place("meadow", 1, 3, 1) + place("meadow", 1, 4, 1)
    # The far portal's mouth must face edge 4: place picks the first legal turn (3), one `rotate`
    # steps to 4.
    s += [
        {"cmd": "select_tile", "id": "rail_tunnel_portal"},
        {"cmd": "hover", "q": 1, "r": 5},
        {"cmd": "wait", "frames": 3},
        {"cmd": "rotate", "dir": "cw"},
        {"cmd": "wait", "frames": 3},
        {"cmd": "place", "q": 1, "r": 5},
    ]
    s += place("rail_straight", 1, 6) + place("rail_straight", 1, 7)
    a, b = world(1, 2), world(1, 5)
    n = math.hypot(b[0] - a[0], b[1] - a[1])
    d = [(b[0] - a[0]) / n, (b[1] - a[1]) / n]
    s += [{"cmd": "wait", "frames": 600}]
    s += views(a, b, d, "rail")
    return s + [{"cmd": "exit"}]


def road():
    src = json.loads((HERE.parent / "elevation" / "smoke.json").read_text())
    s = [c for c in src if c.get("cmd") != "exit"]
    a, b = world(2, 2), world(4, 2)
    s += [{"cmd": "wait", "frames": 600}]
    s += views(a, b, [1.0, 0.0], "road", forward_first=True)
    return s + [{"cmd": "exit"}]


def main():
    for name, s in (("rail", rail()), ("road", road())):
        p = HERE / f"{name}-tunnel-shots.json"
        p.write_text("[\n" + ",\n".join("  " + json.dumps(c) for c in s) + "\n]\n")
        print(f"wrote {p} ({len(s)} commands)")


if __name__ == "__main__":
    main()
