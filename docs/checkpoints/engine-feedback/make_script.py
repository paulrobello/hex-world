"""Writes engine-feedback.json, the native shot script for the engine feedback frame strips.

Run under `--clock fixed` (1/60 s a frame): each command takes one frame, so consecutive
screenshots are 16.7 ms apart and a strip's frame times are exact. strips.py composes the
captures into `docs/checkpoints/engine-feedback/*.jpg`.
"""

import json
from pathlib import Path

CAM = {"cmd": "camera", "pos": [27.7, 38, 40], "look_at": [27.7, 0, 2]}
s: list[dict] = []


def shots(name: str, n: int, every: int = 1) -> None:
    """n screenshots `every` frames apart (a `wait` fills the gap), named name-00 .. name-<n-1>."""
    for i in range(n):
        s.append({"cmd": "screenshot", "name": f"{name}-{i:02d}"})
        if every > 1 and i < n - 1:
            s.append({"cmd": "wait", "frames": every - 1})


def strips(reduced: bool) -> None:
    p = "rm-" if reduced else ""
    # Ghost turn on R: the windmill (six distinct rotations) on the free slot east of the start tile.
    s.extend([{"cmd": "select_tile", "id": "windmill"}, {"cmd": "hover", "q": 1, "r": 0}, {"cmd": "wait", "frames": 40}])
    s.append({"cmd": "rotate", "dir": "cw"})
    shots(f"{p}ghost-turn", 14)
    s.append({"cmd": "wait", "frames": 30})
    # Ghost appears on a newly hovered slot: 150 ms wait, then a 120 ms fade.
    s.append({"cmd": "hover", "q": 0, "r": 1})
    shots(f"{p}ghost-in", 12, 2)
    s.append({"cmd": "hover", "q": 1, "r": 0})
    s.append({"cmd": "wait", "frames": 40})
    # Refused placement: an ocean tile does not fit here; the ghost's edges pulse once (240 ms).
    s.extend([{"cmd": "select_tile", "id": "ocean"}, {"cmd": "wait", "frames": 40}])
    s.append({"cmd": "place", "q": 1, "r": 0})
    shots(f"{p}refused", 16)
    s.extend([{"cmd": "select_tile", "id": "windmill"}, {"cmd": "wait", "frames": 40}])
    # Placement: the commit flash and the staged rise (2 s), every 6 frames (100 ms).
    s.append({"cmd": "place", "q": 1, "r": 0})
    shots(f"{p}rise", 24, 6)
    s.append({"cmd": "wait", "frames": 30})
    # Bulldoze mode on: the free-slot ring leaves and the target arrives (180 ms each).
    s.append({"cmd": "hover", "q": 1, "r": 0})
    s.append({"cmd": "wait", "frames": 10})
    s.append({"cmd": "set_tool", "mode": "bulldoze"})
    shots(f"{p}bulldoze-mode", 14)
    # Bulldoze: the 700 ms sink inside the dust cloud, every 6 frames.
    s.append({"cmd": "bulldoze", "q": 1, "r": 0})
    shots(f"{p}bulldoze", 24, 6)
    s.append({"cmd": "set_tool", "mode": "place"})
    s.append({"cmd": "wait", "frames": 60})


s.extend([CAM, {"cmd": "wait", "frames": 60}, {"cmd": "set_reduced_motion", "on": False}])
strips(False)
s.extend([{"cmd": "set_reduced_motion", "on": True}, {"cmd": "wait", "frames": 5}])
strips(True)
s.append({"cmd": "exit"})
Path(__file__).with_name("engine-feedback.json").write_text(json.dumps(s, indent=1) + "\n")
print(f"{len(s)} commands")
