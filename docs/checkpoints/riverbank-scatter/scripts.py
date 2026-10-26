"""Writes the riverbank scatter checkpoint script (owner 2026-10-26: "riverbank scatter is too
concentrated and repetitive, change scatter to 0-4 things"): five river_straight tiles in a row
along +x beside the start meadow (q = 0, 1, -1, 2, -2 at r = 1, centred at x 13.86, z 24),
photographed from fixed cameras.
Usage: python3 scripts.py <before|after> (writes rs-<which>-stretch.json here)."""

import json
import pathlib
import sys

here = pathlib.Path(__file__).resolve().parent
which = sys.argv[1]
tiles = [("river_straight", q, 1) for q in (0, 1, -1, 2, -2)]
cams = [
    ("stretch", [13.86, 55.0, 104.0], [13.86, -1.0, 24.0]),
    ("close", [13.86, 18.0, 50.0], [13.86, -1.0, 24.0]),
    ("along", [-81.0, 10.0, 38.0], [13.86, -1.0, 24.0]),
    ("far", [13.86, 160.0, 254.0], [13.86, 0.0, 24.0]),
]
if which == "after":
    # Eye level (1.7 m) on the north bank top, looking downstream along the bank (added with the
    # change, so there is no before still).
    cams.append(("eye", [-20.0, 1.7, 32.0], [10.0, -0.9, 28.0]))
out = [{"cmd": "camera", "pos": [0, 160, 190], "look_at": [0, 0, 20]}, {"cmd": "wait", "frames": 30}]
for tid, q, r in tiles:
    out += [
        {"cmd": "select_tile", "id": tid},
        {"cmd": "hover", "q": q, "r": r},
        {"cmd": "place", "q": q, "r": r},
    ]
out += [{"cmd": "wait", "frames": 240}, {"cmd": "dump_state", "name": f"rs-{which}-stretch-state"}]
for name, pos, look in cams:
    out += [
        {"cmd": "camera", "pos": pos, "look_at": look},
        {"cmd": "wait", "frames": 40},
        {"cmd": "photo_export", "scale": 1, "name": f"rs-{which}-{name}"},
        {"cmd": "wait", "frames": 20},
    ]
out.append({"cmd": "exit"})
(here / f"rs-{which}-stretch.json").write_text(json.dumps(out, indent=1) + "\n")

if which == "after":
    # A second, after-only world: the beach-dressing mouth world (meadows, a river bend, both river
    # mouths, river straights and both springs, as docs/checkpoints/beach-dressing/bd-after-mouth.json
    # builds it), photographed from 22 m off each river tile. Cell (q, r) sits at
    # x = 27.71 (q + r / 2), z = 24 r.
    world = json.loads((here.parent / "beach-dressing" / "bd-after-mouth.json").read_text())
    last = max(i for i, c in enumerate(world) if c["cmd"] == "dump_state")
    out = world[:last] + [{"cmd": "dump_state", "name": "rs-after-family-state"}]
    for name, (q, r) in {"bend": (2, 0), "mouth": (3, 0), "straight": (1, 1), "spring": (0, 2),
                         "mouth-l": (-3, 0), "bend-l": (-3, 2)}.items():
        x, z = 27.71 * (q + r / 2), 24.0 * r
        out += [
            {"cmd": "camera", "pos": [x, 26.0, z + 22.0], "look_at": [x, -1.0, z]},
            {"cmd": "wait", "frames": 40},
            {"cmd": "photo_export", "scale": 1, "name": f"rs-after-family-{name}"},
            {"cmd": "wait", "frames": 20},
        ]
    out.append({"cmd": "exit"})
    (here / "rs-after-family.json").write_text(json.dumps(out, indent=1) + "\n")
