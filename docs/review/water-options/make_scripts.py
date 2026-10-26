"""Writes the water-options review scripts (beach.json, river.json, bend60.json) beside this file.

  python3 docs/review/water-options/make_scripts.py

beach.json is the water-foam checkpoint's coast island (docs/checkpoints/wet-band/wet-band.json)
with a 5 s waterline clip; river.json is river_fork's checkpoint layout (straights and a bend on
its arms) with fork and bend stills and a 3 s clip of the fork; bend60.json is river_bend_60's
checkpoint chain with stills of the tight bend. shots.sh runs them once per
option with `--water_look trail=<x>,churn=<g>`, so every option shares the cameras and the clock."""
import json
import pathlib

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def cam(pos, look):
    return [{"cmd": "camera", "pos": pos, "look_at": look}, {"cmd": "wait", "frames": 30}]


def still(name, pos, look):
    return cam(pos, look) + [{"cmd": "screenshot", "name": name}]


def clip(name, pos, look, frames):
    return cam(pos, look) + [{"cmd": "screenshot", "name": f"{name}-{i:03d}"} for i in range(frames)]


def place(layout):
    out = []
    for tile, cells in layout:
        out.append({"cmd": "select_tile", "id": tile})
        out += [{"cmd": "place", "q": q, "r": r} for q, r in cells]
    return out + [{"cmd": "hover", "q": 9, "r": -9}, {"cmd": "wait", "frames": 900}]


def beach():
    src = json.loads((ROOT / "docs/checkpoints/wet-band/wet-band.json").read_text())
    end = next(i for i, c in enumerate(src) if c.get("cmd") == "wait" and c.get("frames") == 150)
    out = list(src[: end + 1])
    out += clip("wo-beach", [-8, 5, -33], [-13.86, -1.3, -24], 150)
    return out + [{"cmd": "exit"}]


def river():
    out = [{"cmd": "camera", "pos": [36, 62, 48], "look_at": [27.7, 0, 0]}]
    out += place((
        ("river_fork", [(1, 0)]),
        ("oak", [(1, 1)]),
        ("meadow", [(2, -1)]),
        ("river_straight", [(2, 0), (1, -1), (3, 0)]),
        ("river_bend", [(0, 1)]),
    ))
    out += still("wo-fork-top", [27.8, 38, 14], [27.7, 0, 0])
    out += still("wo-fork-oblique", [36.7, 7.5, 7], [27.7, -1.4, 0])
    out += still("wo-bend-top", [13.86, 30, 30], [13.86, 0, 24])
    out += still("wo-bend-oblique", [6, 7.5, 16], [13.86, -1.4, 24])
    out += clip("wo-fork-motion", [36.7, 7.5, 7], [27.7, -1.4, 0], 90)
    return out + [{"cmd": "exit"}]


def bend60():
    # docs/checkpoints/river_bend_60/play.json's chain: bend_60 at (1, 0), a straight on each arm.
    out = [{"cmd": "camera", "pos": [22, 62, 78], "look_at": [34, 0, 12]}]
    out += place((
        ("river_bend_60", [(1, 0)]),
        ("river_straight", [(2, 0)]),
        ("river_bend", [(1, 1)]),
        ("river_straight", [(0, 2)]),
        ("meadow", [(0, 1), (1, -1), (2, -1), (0, -1), (-1, 1)]),
    ))
    out += still("wo-bend60-top", [36, 40, 34], [37, 0, 6])
    out += still("wo-bend60-oblique", [50.4, 8, 19], [40, -1.4, 6])
    return out + [{"cmd": "exit"}]


for name, cmds in (("beach", beach()), ("river", river()), ("bend60", bend60())):
    (HERE / f"{name}.json").write_text("[\n" + ",\n".join("  " + json.dumps(c) for c in cmds) + "\n]\n")
    print(f"wrote {name}.json ({len(cmds)} commands)")
