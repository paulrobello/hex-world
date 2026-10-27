"""outcrop_granite: a low whaleback granite outcrop about 8 x 5 m and 1.2 m high whose sheet joints split it
into broad slabs (ref-2), in rock_small's weathered granite with moss and lichen
(refs: assets-source/refs/boulders, owner A25). Scripted route, prop kind (LOD0 <= 1,500 tris), a
runtime scatter prop (owner 2026-10-26: rocks are scatter, never baked into a tile). The pipeline
is `_boulder.build`: make kit-build PIECE=outcrop_granite."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _boulder  # noqa: E402
from _boulder import Joint, Stone  # noqa: E402,F401

TEXEL_MIN = 104.0  # owner A25: a unique bake at about 128 px/m
_boulder.build("outcrop_granite", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
    stones=[Stone(semi=(4.30, 2.50, 1.38), centre=(0.0, 0.0, -0.15), planes=12, keep=(0.86, 0.95),
                  top_el=(1.3, 1.5), tilt=(0.10, 0.06))],
    bury=-0.32, lod_tris=(1400, 600, 180), atlas=1024, normal_size=512, orm_size=512, mask_size=256,
    texel_min=TEXEL_MIN, lump=0.10,
    joints=(Joint(a=(-3.1, 0.35), b=(2.9, 0.55), depth=0.30, width=0.26, step=0.18),
            Joint(a=(0.6, 0.45), b=(0.9, -2.5), depth=0.26, width=0.22, step=0.14),
            Joint(a=(-1.5, 0.40), b=(-1.2, 2.4), depth=0.22, width=0.20, step=0.12)))
