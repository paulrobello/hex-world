"""Draw census of the beach/ocean scatter checkpoint's state dumps: per dump, the draw calls over every
pass and cascade (multidraw sets + batchable bins + unbatchable + non-mesh, as
docs/checkpoints/scatter/check.mjs counts them), the main opaque pass alone, and the scatter's
(mesh, material) pairs per LOD (limit 60 each, 180 in all) with its drawn instances by piece.
Usage: python3 census.py <state.json>..."""

import json
import pathlib
import sys

for path in sys.argv[1:]:
    d = json.loads(pathlib.Path(path).read_text())
    passes = d["diag"]["passes"]
    draws = sum(p["multidraw_sets"] + p["batchable_bins"] + p["unbatchable"] + p["non_mesh"] for p in passes)
    opaque = next((p for p in passes if p["view"] == "main" and p["phase"] == "opaque"), None)
    s = d.get("scatter", {})
    print(f"{pathlib.Path(path).name}: {draws} draws over {len(passes)} passes; main opaque {opaque}; "
          f"scatter pairs by LOD {s.get('mesh_material_pairs_by_lod')} ({s.get('mesh_material_pairs')} in all), "
          f"drawn {s.get('drawn')} {json.dumps(s.get('drawn_by_piece', {}))}")
