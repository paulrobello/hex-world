"""boulder_granite_b: a rounded granite boulder about 2.6 x 2.2 m and 1.4 m proud of the ground (ref-1, front left), in rock_small's weathered granite with moss and lichen
(refs: assets-source/refs/boulders, owner A25). Scripted route, prop kind (LOD0 <= 1,500 tris), a
runtime scatter prop (owner 2026-10-26: rocks are scatter, never baked into a tile). The pipeline
is `_boulder.build`: make kit-build PIECE=boulder_granite_b."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _boulder  # noqa: E402
from _boulder import Joint, Stone  # noqa: E402,F401

TEXEL_MIN = 104.0  # owner A25: a unique bake at about 128 px/m
_boulder.build("boulder_granite_b", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
    stones=[Stone(semi=(1.25, 1.07, 1.10), centre=(0.0, 0.0, 0.33), planes=12, keep=(0.86, 0.95),
                  tilt=(0.12, 0.05))],
    bury=-0.40, lod_tris=(1400, 600, None), atlas=1024, normal_size=512, orm_size=512, mask_size=256,
    texel_min=TEXEL_MIN, lump=0.08)
