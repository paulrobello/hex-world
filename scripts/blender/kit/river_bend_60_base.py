"""river_bend_60_base: the channel-cut base for river lanes on edges 0 and 1 (river_bend_60, the tight
bend). meadow_base with the 7.42 m bend arc carved in between the two river_edge stubs: one
continuous cap + bank + bed surface, the soil skirt everywhere but the channel spans, a notch for the
river_edge stub at each lane edge. Scripted route (PRD section 11.1). Profile and contract:
_river.py; centre line and the stepped inner offsets: _river_bend_60.py."""
import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _river  # noqa: E402
import _river_bend_60 as bend  # noqa: E402
import _river_flat  # noqa: E402,F401  (the flattened meadow relief, owner D6-Q1)

PIECE = "river_bend_60_base"
# Radial rows between the joints, meadow lattice spacing and skirt rows per LOD (ring and seam rows
# fixed). Chord sag on the outer bank sets the row count (the outer waterline runs at 11.4 m from the
# arc centre); the inner offsets step down to every second to eighth row (`bend.steps`). LOD2 keeps
# river_bend_base's eight half-offsets and uses the lane-cut LOD2 ceiling.
ROWS = (40, 20, 20)
SPACING = (0.88, 1.8, None)
SKIRT_ROWS = (4, 2, 1)
LOD2_HALF = (0.0, 0.8, 2.0, _river.KINKS[0], 3.5, _river.KINKS[1], 5.7, _river.BANK_TOP)
# LOD2's inner offset at -5.7 lies on the dry bank (0.10 m tolerance), so it takes every fourth row
# rather than the sag budget's second: about 1% of LOD2 under the 20% ceiling.
LOD2_INNER = [(o, 4 if abs(o + 5.7) < 1e-9 else st) for o, st in bend.steps(_river.symmetric(LOD2_HALF), ROWS[2])]
INNER = (bend.steps(_river.OFFS_FINE, ROWS[0]), bend.steps(_river.OFFS_FINE, ROWS[1]), LOD2_INNER)

_river.build_base(PIECE, "bend60", ROWS, SPACING, SKIRT_ROWS, __file__, INNER)

# The tile's network polyline (data/tiles/river_bend_60.ron) beside the bed it was built against.
report = pathlib.Path(sys.argv[sys.argv.index("--") + 1]) / "bake-report.json"
fields = json.loads(report.read_text())
fields["centreLine"] = {"radiusM": round(bend.R60, 4), "centreRuntimeXZ": [round(bend.C60[0], 4), round(-bend.C60[1], 4)],
                        "sweepDeg": 120.0, "stepDeg": bend.ARC_STEP_DEG, "networkPoints": bend.network_points()}
report.write_text(json.dumps(fields, indent=2, sort_keys=True) + "\n")
