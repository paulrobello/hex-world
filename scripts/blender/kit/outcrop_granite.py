"""outcrop_granite: a low whaleback granite outcrop about 8 x 5 m and 1.2 m high whose sheet joints split it
into broad slabs (ref-2), in rock_small's weathered granite with moss and lichen
(refs: assets-source/refs/boulders, owner A25). Scripted route, prop kind (LOD0 <= 1,500 tris), a
runtime scatter prop (owner 2026-10-26: rocks are scatter, never baked into a tile). The pipeline
is `_boulder.build`: make kit-build PIECE=outcrop_granite.

Redo (owner 2026-10-30: "the lines on it make it too recognizable as stamped"). `_boulder.shape_high`
carves a `Joint` as a straight V-groove, and the outcrop's three read as a ruler-straight T on every
instance. `_outcrop.install` replaces `_boulder.shape_high` and wraps `_boulder.sparser_moss` with the
wandering `Fracture`s, near-fracture refinement, top swell and varied lichen that outcrop_granite_b
shares (`_outcrop.py`), so `_boulder.py`, `_rock.py` and the four boulders are unchanged."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _boulder  # noqa: E402
import _outcrop  # noqa: E402
from _boulder import Stone  # noqa: E402
from _outcrop import Fracture  # noqa: E402

TEXEL_MIN = 104.0  # owner A25: a unique bake at about 128 px/m
SEED = "outcrop_granite-fractures"

FRACTURES = [
    # The main joint, about 11 degrees off the long axis, ending well inside both rims.
    Fracture(a=(-3.25, -0.45), b=(2.75, 0.70), depth=0.14, width=0.13, step=0.07, fade=2.2,
             wobble=0.16, ends=(0.10, 0.22)),
    # A branch toward the -y rim, 110 degrees off the main joint; the +x slab stands higher.
    Fracture(a=(0.0, 0.0), b=(1.35, -1.80), depth=0.09, width=0.09, step=0.05, fade=1.6,
             wobble=0.20, ends=(0.0, 0.35), parent=0, at=0.58),
    # A shorter branch toward the +y rim, 57 degrees off the main joint.
    Fracture(a=(0.0, 0.0), b=(-0.85, 1.75), depth=0.07, width=0.075, step=0.04, fade=1.2,
             wobble=0.16, ends=(0.0, 0.40), parent=0, at=0.27),
    # A hairline at the -x end, dying out at both ends.
    Fracture(a=(-2.60, -1.25), b=(-1.35, -1.75), depth=0.035, width=0.04, step=0.0, fade=1.0,
             wobble=0.20, ends=(0.30, 0.30)),
]

PATHS = _outcrop.install("outcrop_granite", FRACTURES, SEED)

_boulder.build("outcrop_granite", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
    stones=[Stone(semi=(4.30, 2.50, 1.38), centre=(0.0, 0.0, -0.15), planes=12, keep=(0.86, 0.95),
                  top_el=(1.3, 1.5), tilt=(0.10, 0.06))],
    bury=-0.32, lod_tris=(1398, 600, 180), atlas=1024, normal_size=512, orm_size=512, mask_size=256,
    texel_min=TEXEL_MIN, lump=0.10, joints=tuple(FRACTURES))
