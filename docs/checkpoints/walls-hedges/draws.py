"""Overview draw census of the B2 draw worlds, with and without the field boundaries: the sum over every
pass and cascade of multidraw sets, batchable bins, unbatchable and non-mesh draws, as
docs/checkpoints/scatter/check.mjs counts it (PRD §10, the 500-draw overview budget).
Usage: python3 docs/checkpoints/walls-hedges/draws.py <dump.json> [<dump.json> ...]"""
import json
import sys


def census(path: str) -> dict:
    d = json.load(open(path))
    passes = (d.get("diag") or {}).get("passes") or []
    draws = sum(p["multidraw_sets"] + p["batchable_bins"] + p["unbatchable"] + p["non_mesh"] for p in passes)
    by_pass = [f'{p["view"]}/{p["phase"]}:{p["multidraw_sets"]}+{p["batchable_bins"]}+{p["unbatchable"]}+{p["non_mesh"]}'
               for p in passes]
    play = d.get("play") or {}
    return {"dump": path, "tiles": d.get("tiles"), "draws": draws, "passes": by_pass,
            "waiting": play.get("waiting"), "packs": d.get("packs")}


for path in sys.argv[1:]:
    c = census(path)
    print(json.dumps(c))
