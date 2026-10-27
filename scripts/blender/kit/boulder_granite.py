"""boulder_granite: a low, squat granite boulder about 2.2 x 1.8 m and 1.0 m proud of the ground (ref-1, front right), in rock_small's weathered granite with moss and lichen
(refs: assets-source/refs/boulders, owner A25). Scripted route, prop kind (LOD0 <= 1,500 tris), a
runtime scatter prop (owner 2026-10-26: rocks are scatter, never baked into a tile). The pipeline
is `_boulder.build`: make kit-build PIECE=boulder_granite."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _boulder  # noqa: E402
from _boulder import Joint, Stone  # noqa: E402,F401

TEXEL_MIN = 104.0  # owner A25: a unique bake at about 128 px/m
_boulder.build("boulder_granite", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
    stones=[Stone(semi=(1.12, 0.93, 0.85), centre=(0.0, 0.0, 0.26), planes=11, keep=(0.80, 0.92),
                  top_el=(1.2, 1.45), tilt=(0.10, 0.04))],
    bury=-0.40, lod_tris=(1400, 600, None), atlas=512, normal_size=512, orm_size=512, mask_size=256,
    texel_min=TEXEL_MIN, lump=0.06)
