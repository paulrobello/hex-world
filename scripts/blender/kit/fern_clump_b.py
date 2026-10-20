"""fern_clump_b: a young fern, about 0.45 m tall and 0.7 m across, a narrow upright shuttlecock of
twelve fronds that stand steep and only bow at their tips (refs: assets-source/refs/forest, "ferns").
Same frond tiles, palette, card fold, sink, wind and LOD scheme as fern_clump; the pipeline is
`_ferns_var.build`. Scripted route, prop kind (LOD0 <= 1,500 tris), numpy-drawn tiles, no Cycles
bake: make kit-build PIECE=fern_clump_b."""
import math
import pathlib
import random
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _ferns_var  # noqa: E402
import _props  # noqa: E402

PIECE = "fern_clump_b"
FRONDS, SINK = 12, 0.08
rng = random.Random(PIECE)
fronds = []
for i in range(FRONDS):
    ring = (i % 3) / 2.0  # 0 inner (youngest, near vertical), 1 outer
    fronds.append({"a": i * 2.39996 + rng.uniform(-0.25, 0.25), "r0": 0.02 + 0.02 * ring,
                   "el0": math.radians(rng.uniform(86, 89) - 12 * ring),
                   "el1": math.radians(rng.uniform(52, 60) - 30 * ring),
                   "L": rng.uniform(0.95, 1.0) * 0.6 * (1.0 - 0.2 * ring),
                   "roll": rng.uniform(-0.18, 0.18), "tile": i % 3,
                   "phase": _props.unit_hash(PIECE, "frond", i), "sway": rng.uniform(0.9, 1.0)})

_ferns_var.build(PIECE, pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, fronds=fronds, sink=SINK,
                 round_r=0.35, core_r=0.195, core_h=0.35, height=(0.4, 0.52), reach=(0.25, 0.45))
