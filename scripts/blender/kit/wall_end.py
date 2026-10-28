"""wall_end: the 2 m end: one arm from the hex corner (the pivot) along the edge, stopped square at the corner
in larger stones, where a wall boundary stops (or meets a hedge). Dry granite field wall, 1.1 m tall and 0.7 m
wide at the foot, with an upright cope (B2, owner A31; refs: assets-source/refs/walls_hedges ref-1). Prop
kind; the pipeline is `_drystone.build` (numpy-drawn granite stone faces, no Cycles bake). Placed by the
runtime on field-boundary edges (`hex_core::boundary`). Scripted route, headless: make kit-build
PIECE=wall_end."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _drystone  # noqa: E402

_drystone.build("wall_end", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, kind="corner", arms=(0.0,))
