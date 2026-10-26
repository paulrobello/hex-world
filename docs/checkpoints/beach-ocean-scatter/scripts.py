"""Writes the beach/ocean scatter checkpoint scripts (owner 2026-10-26: "scatter type items like rocks
and reeds should not be built into the tile"): the beach-dressing coast island ring (beach,
beach_inner and beach_outer around a meadow, ocean outside), built exactly as
docs/checkpoints/beach-dressing/bd-after-coast.json does, then photographed from fixed cameras over a
beach stretch and an ocean stretch, close and at play distance, plus two far views (about 150 m and
400 m from the coast) for the LOD switch. Usage: python3 scripts.py <before|after> (writes
bos-<which>.json here)."""

import json
import pathlib
import sys

here = pathlib.Path(__file__).resolve().parent
src = here.parent / "beach-dressing"
which = sys.argv[1]
cams = [
    # Beach stretch at play distance: the straight beach at (0, 2) and its corner neighbours.
    ("beach-stretch", [27.7, 26.0, 88.0], [27.7, -1.0, 48.0]),
    # The straight beach's grass line at 8 m, where the pebbles lie.
    ("beach-close", [24.0, 4.0, 46.0], [27.7, -0.3, 38.0]),
    # Ocean stretch: the open sea north of the island, looking down through the water.
    ("ocean-stretch", [-27.7, 24.0, -18.0], [-27.7, -4.0, -52.0]),
    ("ocean-close", [-27.7, 6.0, -38.0], [-27.7, -4.0, -48.0]),
    # Far views of the same coast: about 150 m and 400 m from the beach stretch.
    ("far-150", [27.7, 60.0, 185.0], [27.7, -1.0, 48.0]),
    ("far-400", [27.7, 140.0, 425.0], [27.7, -1.0, 48.0]),
]
script = json.loads((src / "bd-after-coast.json").read_text())
last = max(i for i, c in enumerate(script) if c["cmd"] == "dump_state")
# The source's window screenshot and its dumps are not this sheet's frames (window captures come back
# black under load; every frame here is an offscreen photo_export).
out = [c for c in script[:last] if c["cmd"] not in ("screenshot", "dump_state")]
out.append({"cmd": "dump_state", "name": f"bos-{which}-state"})
for name, pos, look in cams:
    out += [
        {"cmd": "camera", "pos": pos, "look_at": look},
        {"cmd": "wait", "frames": 40},
        {"cmd": "photo_export", "scale": 1, "name": f"bos-{which}-{name}"},
        {"cmd": "wait", "frames": 20},
    ]
    # The after run also dumps each camera's draw census (the before run, shot first, dumped only
    # the two states below).
    if which == "after":
        out.append({"cmd": "dump_state", "name": f"bos-{which}-{name}-state"})
out.append({"cmd": "dump_state", "name": f"bos-{which}-end-state"})
out.append({"cmd": "exit"})
(here / f"bos-{which}.json").write_text(json.dumps(out, indent=1) + "\n")

# A second script, shot after the first: straight down on the bay beach_inner at (1, 0), the tile
# that keeps three pebble clusters in the after layout (bos-after-state.json), from 24 m up. The
# before side is shot with the before binary and assets (`git archive 2dc2aeb assets`).
top = [c for c in script[:last] if c["cmd"] not in ("screenshot", "dump_state")]
top += [
    {"cmd": "camera", "pos": [27.7, 24.0, 1.0], "look_at": [27.7, -0.5, 0.0]},
    {"cmd": "wait", "frames": 40},
    {"cmd": "photo_export", "scale": 1, "name": f"bos-{which}-pebbles-top"},
    {"cmd": "wait", "frames": 20},
    {"cmd": "exit"},
]
(here / f"bos-{which}-top.json").write_text(json.dumps(top, indent=1) + "\n")

# A third, frameless script: each camera's draw census alone (a dump_state per camera, as the after
# run's), so the before set, shot first without them, compares camera for camera.
census = [c for c in script[:last] if c["cmd"] not in ("screenshot", "dump_state")]
for name, pos, look in cams:
    census += [
        {"cmd": "camera", "pos": pos, "look_at": look},
        {"cmd": "wait", "frames": 60},
        {"cmd": "dump_state", "name": f"bos-{which}-{name}-state"},
    ]
census.append({"cmd": "exit"})
if which == "before":
    (here / f"bos-{which}-census.json").write_text(json.dumps(census, indent=1) + "\n")
