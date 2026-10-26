"""Draw counts per render pass, before against after, from the shot runs' state dumps.

  python3 docs/checkpoints/road_bend_60-trim/draws.py

Reads `diag.passes` from before/ and after/ `trk-state.json` (kit scenario, the whole kit layout in
view) and `trp-state.json` (play, the bend between two straights and meadows) and prints, per pass
(view, phase), multidraw sets, batchable bins, unbatchable draws and non-mesh draws (in play, one
per non-empty kit (slot, LOD) draw of each mesh family), with the change.
"""
import json
import pathlib

DOCS = pathlib.Path(__file__).resolve().parent
KEYS = ("multidraw_sets", "batchable_bins", "unbatchable", "non_mesh")

for name in ("trk-state", "trp-state"):
    rows = {}
    for side in ("before", "after"):
        state = json.loads((DOCS / side / f"{name}.json").read_text())
        for p in state["diag"]["passes"] or []:
            rows.setdefault((p["view"], p["phase"]), {})[side] = tuple(p.get(k, 0) for k in KEYS)
    print(f"{name}: (multidraw sets, batchable bins, unbatchable, non-mesh) before -> after")
    for (view, phase), v in rows.items():
        b, a = v.get("before"), v.get("after")
        mark = "" if a == b else "   <- changed"
        print(f"  {view:>10} {phase:<22} {b} -> {a}{mark}")
