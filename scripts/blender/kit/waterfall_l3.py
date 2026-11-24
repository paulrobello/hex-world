"""waterfall_l3: the 12 m fall face: the lip and two ledges, three tiers (RQ2 O2, ref-7). The family pipeline is
`_waterfall.build` (rivers-at-levels spec section 4; refs assets-source/refs/waterfall):
make kit-build PIECE=waterfall_l3."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _waterfall

DROP = 3
TEXEL_MIN = 55.0  # kit_gate WATERFALL_TEXEL_MIN_PX_PER_M: LOD0's measured median, rounded down to 5

_waterfall.build("waterfall_l3", DROP, pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
                 texel_min=TEXEL_MIN)
