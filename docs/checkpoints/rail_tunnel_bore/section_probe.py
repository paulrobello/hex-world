"""Measure the rail tunnel's free space against the cover level and the approved portal mound.

Run headless (no build, nothing written outside this folder):
  /Applications/Blender.app/Contents/MacOS/Blender -b --factory-startup --python-exit-code 1 \
      -P docs/checkpoints/rail_tunnel_bore/section_probe.py
Writes section-probe.json beside this file. The portal numbers come from the generator's own module
(`_tunnel_portal_rail`, which sets the approved road helper's globals at import), the vehicle envelopes
from the committed clearance record of the approved portal (`docs/checkpoints/rail_tunnel_portal/
clearance.json`, made from the shipped train GLBs)."""
import json
import math
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(REPO / "scripts/blender"))
sys.path.insert(0, str(REPO / "scripts/blender/kit"))
import _tunnel_portal_rail as r  # noqa: E402

tp = r.tp
A, J, CROWN, RING = tp.A, tp.J, tp.CROWN, tp.RING
X_F, X_B, X_E = tp.X_F, tp.X_B, tp.X_E
COVER = r.COVER  # the cover meadow's ring level over the portal level (one LEVEL_STEP)
RELIEF_MIN = -0.25  # meadow_base relief dips (`_tunnel_portal.meadow_relief`)


def first_below(level, s=0.0, n=4800):
    for k in range(n + 1):
        x = X_B + (X_E - X_B) * k / n
        if r.mound(x, s, True) < level:
            return round(x, 4)
    return None


long_section = [[round(X_F + (X_E - X_F) * k / 200, 4), round(r.mound(X_F + (X_E - X_F) * k / 200, 0.0, True), 4)]
                for k in range(201)]
intrados = [[round(A * math.cos(math.pi * k / 90), 4), round(J + A * math.sin(math.pi * k / 90), 4)] for k in range(91)]
cl = json.loads((REPO / "docs/checkpoints/rail_tunnel_portal/clearance.json").read_text())
vehicles = {}
for name, v in cl["vehicles"].items():
    env = v["envelope"]
    over = [p for p in env if p[1] > COVER]
    vehicles[name] = {"topM": max(p[1] for p in env), "overCoverM": round(max(p[1] for p in env) - COVER, 3),
                      "overReliefDipM": round(max(p[1] for p in env) - COVER - RELIEF_MIN, 3),
                      "maxHalfWidthOverCoverM": max((p[0] for p in over), default=None), "envelope": env}
out = {
    "section": {"halfSpanM": A, "springingM": J, "crownM": CROWN, "ringM": RING, "railTopM": r.RAIL_TOP},
    "cover": {"levelM": COVER, "reliefMinM": COVER + RELIEF_MIN},
    "freeSpaceOverCover": {
        "crownOverCoverM": round(CROWN - COVER, 3),
        "crownOverReliefDipM": round(CROWN - COVER - RELIEF_MIN, 3),
        "intradosHalfWidthAtCoverM": round(math.sqrt(A * A - (COVER - J) ** 2), 4),
        "intradosHalfWidthAtReliefDipM": round(math.sqrt(A * A - (COVER + RELIEF_MIN - J) ** 2), 4),
    },
    "portalMound": {
        "xFace": round(X_F, 4), "xBack": round(X_B, 4), "xEdge0": round(X_E, 4),
        "moundAtBackPlaneM": round(r.mound(X_B, 0.0, True), 4), "moundAtEdge0M": round(r.mound(X_E, 0.0, True), 4),
        "moundBelowCrownFromX": first_below(CROWN),
        "moundBelowCrownPlusLiningCoverFromX": first_below(CROWN + r.LINING_COVER),
        "moundBelowExtradosFromX": first_below(CROWN + RING),
        "liningCoverM": r.LINING_COVER,
        "longSectionS0": long_section,
    },
    "vehicles": vehicles,
    "intrados": intrados,
}
(HERE / "section-probe.json").write_text(json.dumps(out, indent=1) + "\n")
summary = {k: v for k, v in out.items() if k not in ("intrados",)}
summary["portalMound"] = {k: v for k, v in out["portalMound"].items() if k != "longSectionS0"}
summary["vehicles"] = {k: {kk: vv for kk, vv in v.items() if kk != "envelope"} for k, v in vehicles.items()}
print(json.dumps(summary, indent=1))
