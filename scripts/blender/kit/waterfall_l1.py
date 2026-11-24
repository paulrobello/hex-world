"""waterfall_l1: the 4 m fall face: one sheet over a natural lip (RQ2 O1, ref-1). The family pipeline is
`_waterfall.build` (rivers-at-levels spec section 4; refs assets-source/refs/waterfall):
make kit-build PIECE=waterfall_l1."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _waterfall

DROP = 1
TEXEL_MIN = 100.0  # kit_gate WATERFALL_TEXEL_MIN_PX_PER_M: LOD0's measured median, rounded down to 5

_waterfall.build("waterfall_l1", DROP, pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
                 texel_min=TEXEL_MIN)
