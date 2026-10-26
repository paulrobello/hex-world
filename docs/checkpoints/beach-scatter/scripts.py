"""Writes the beach scatter checkpoint scripts (owner 2026-10-21: "0 to 6 is plenty") from the
beach-dressing worlds: the coast island ring (beach, beach_inner, beach_outer) and the two river
mouths, each built exactly as docs/checkpoints/beach-dressing/bd-after-*.json does, then photographed
from fixed cameras. Usage: python3 scripts.py <before|after> (writes bs-<which>-<world>.json here)."""

import json
import pathlib
import sys

here = pathlib.Path(__file__).resolve().parent
src = here.parent / "beach-dressing"
which = sys.argv[1]
cams = {
    "coast": [
        ("overview", [0.0, 150.0, 175.0], [0.0, 0.0, 12.0]),
        ("north", [0.0, 70.0, 100.0], [0.0, 0.0, 40.0]),
        ("headland-play", [95.0, 22.0, 62.0], [62.0, -1.5, 18.0]),
        ("bay-play", [48.0, 20.0, -35.0], [27.7, -1.0, 0.0]),
    ],
    "mouth": [
        ("straight-play", [120.0, 24.0, -10.5], [92.2, -0.5, -26.8]),
        ("mouth-play", [106.5, 24.0, 13.5], [78.8, -0.4, -2.5]),
        ("mouth-l-play", [-106.5, 24.0, 13.5], [-78.8, -0.4, -2.5]),
    ],
}
for world, shots in cams.items():
    script = json.loads((src / f"bd-after-{world}.json").read_text())
    last = max(i for i, c in enumerate(script) if c["cmd"] == "dump_state")
    out = script[:last]
    out.append({"cmd": "dump_state", "name": f"bs-{which}-{world}-state"})
    for name, pos, look in shots:
        out += [
            {"cmd": "camera", "pos": pos, "look_at": look},
            {"cmd": "wait", "frames": 40},
            {"cmd": "photo_export", "scale": 1, "name": f"bs-{which}-{name}"},
            {"cmd": "wait", "frames": 20},
        ]
    out.append({"cmd": "exit"})
    (here / f"bs-{which}-{world}.json").write_text(json.dumps(out, indent=1) + "\n")
