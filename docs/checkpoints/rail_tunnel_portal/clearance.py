"""The train clearance section's data: each committed vehicle's LOD0 cross-section (its vertices on the
(|y|, height over the tile) plane, the rail top at 0.46 m) as an upper envelope in 2 cm bins, beside the
generator's own clearance numbers from the bake report. sheet.py draws it against the intrados.

  "$BLENDER" -b --factory-startup --python-exit-code 1 -P docs/checkpoints/rail_tunnel_portal/clearance.py

Writes docs/checkpoints/rail_tunnel_portal/clearance.json."""
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "scripts/blender"))
sys.path.insert(0, str(REPO / "scripts/blender/kit"))

import _tunnel_portal_rail as tpr  # noqa: E402

out = {"railTop": tpr.RAIL_TOP, "A": tpr.A, "J": tpr.J, "ring": tpr.RING, "crown": tpr.CROWN, "faceTop": tpr.FACE_TOP,
       "top": tpr.TOP, "vehicles": {}}
for name, sha, yz in tpr.train_vertices():
    env = {}
    for y, z in yz:
        k = int(y / 0.02)
        env[k] = max(env.get(k, 0.0), float(z))
    c = tpr.intrados_clearance(yz)
    i = int(c.argmin())
    out["vehicles"][name] = {"sha256": sha, "envelope": [[round(k * 0.02 + 0.01, 3), round(z, 3)] for k, z in sorted(env.items())],
                             "minRadialClearanceM": round(float(c[i]), 4),
                             "at": [round(float(yz[i, 0]), 3), round(float(yz[i, 1]), 3)]}
path = REPO / "docs/checkpoints/rail_tunnel_portal/clearance.json"
path.write_text(json.dumps(out, indent=1) + "\n")
print(f"wrote {path}")
