"""fern_clump_c: a wide, low, spreading fern clump, about 0.45 m tall and 2 m across: three crowns
grown together (one at the pivot, two smaller ones 0.3 m off it), their fronds leaning out low and
their tips arching down onto the litter (refs: assets-source/refs/forest, "ferns", the sprawling
clumps around the fallen log in ref-1). Same frond tiles, palette, card fold, sink, wind and LOD
scheme as fern_clump; the pipeline is `_ferns_var.build`. Scripted route, prop kind (LOD0 <= 1,500
tris), numpy-drawn tiles, no Cycles bake: make kit-build PIECE=fern_clump_c.

The collider stays fern_clump's 0.3 m core, so the family's scatter footprint (the group's largest)
and its spacing on existing forest tiles do not change."""
import math
import pathlib
import random
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _ferns_var  # noqa: E402
import _props  # noqa: E402

PIECE = "fern_clump_c"
SINK = 0.08
# (crown centre in Blender x, y; fronds; length scale)
CROWNS = (((0.0, 0.0), 14, 1.0), ((0.26, 0.15), 8, 0.8), ((-0.18, -0.24), 8, 0.85))
rng = random.Random(PIECE)
fronds = []
for c, n, scale in CROWNS:
    for i in range(n):
        ring = (i % 3) / 2.0  # 0 inner, 1 outer
        k = len(fronds)
        fronds.append({"a": i * 2.39996 + rng.uniform(-0.3, 0.3), "c": c, "r0": 0.03 + 0.03 * ring,
                       "el0": math.radians(rng.uniform(62, 70) - 20 * ring),
                       "el1": math.radians(rng.uniform(-8, 4) - 26 * ring),
                       "L": rng.uniform(0.92, 1.0) * 0.95 * scale * (1.0 - 0.15 * ring),
                       "roll": rng.uniform(-0.18, 0.18), "tile": k % 3,
                       "phase": _props.unit_hash(PIECE, "frond", k), "sway": rng.uniform(0.9, 1.0)})

_ferns_var.build(PIECE, pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, fronds=fronds, sink=SINK,
                 round_r=1.0, core_r=0.295, core_h=0.4, height=(0.38, 0.52), reach=(0.9, 1.25))
