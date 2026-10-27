"""boulder_granite_d: a big rounded granite boulder about 4.5 x 3.5 m and 2.4 m proud of the ground (ref-1, back), in rock_small's weathered granite with moss and lichen
(refs: assets-source/refs/boulders, owner A25). Scripted route, prop kind (LOD0 <= 1,500 tris), a
runtime scatter prop (owner 2026-10-26: rocks are scatter, never baked into a tile). The pipeline
is `_boulder.build`: make kit-build PIECE=boulder_granite_d."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _boulder  # noqa: E402
from _boulder import Joint, Stone  # noqa: E402,F401

TEXEL_MIN = 104.0  # owner A25: a unique bake at about 128 px/m
_boulder.build("boulder_granite_d", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
    stones=[Stone(semi=(2.22, 1.72, 1.90), centre=(0.0, 0.0, 0.50), planes=12, keep=(0.86, 0.95),
                  tilt=(0.15, 0.06))],
    bury=-0.40, lod_tris=(1400, 600, 180), atlas=1024, normal_size=512, orm_size=512, mask_size=256,
    texel_min=TEXEL_MIN, lump=0.12)
