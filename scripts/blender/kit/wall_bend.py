"""wall_bend: the 120 degree bend: two 2 m arms from the hex corner (the pivot) at 0 and 120 degrees, their
faces mitred round the corner, where a wall boundary turns from one tile edge onto the next. Dry granite field
wall, 1.1 m tall and 0.7 m wide at the foot, with an upright cope (B2, owner A31; refs:
assets-source/refs/walls_hedges ref-1). Prop kind; the pipeline is `_drystone.build` (numpy-drawn granite
stone faces, no Cycles bake). Placed by the runtime on field-boundary edges (`hex_core::boundary`). Scripted
route, headless: make kit-build PIECE=wall_bend."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _drystone  # noqa: E402

_drystone.build("wall_bend", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, kind="corner", arms=(0.0, 120.0))
