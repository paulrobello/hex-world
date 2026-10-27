"""boulder_granite_c: a split granite wedge about 3.4 x 2.4 m and 2.0 m high at its raised end (ref-1, right), in rock_small's weathered granite with moss and lichen
(refs: assets-source/refs/boulders, owner A25). Scripted route, prop kind (LOD0 <= 1,500 tris), a
runtime scatter prop (owner 2026-10-26: rocks are scatter, never baked into a tile). The pipeline
is `_boulder.build`: make kit-build PIECE=boulder_granite_c."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _boulder  # noqa: E402
from _boulder import Joint, Stone  # noqa: E402,F401

TEXEL_MIN = 104.0  # owner A25: a unique bake at about 128 px/m
_boulder.build("boulder_granite_c", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
    stones=[Stone(semi=(1.77, 1.30, 1.42), centre=(0.0, 0.0, 0.72), planes=8, keep=(0.66, 0.80),
                  top_el=(1.0, 1.25), tilt=(0.95, 0.05))],
    bury=-0.40, lod_tris=(1400, 600, 180), atlas=1024, normal_size=512, orm_size=512, mask_size=256,
    texel_min=TEXEL_MIN, lump=0.05)
