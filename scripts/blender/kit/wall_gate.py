"""wall_gate: the 4 m gateway: two short wall stubs, two tall granite gateposts and a closed timber five-bar
gate between them (ref-1), one per farm. Dry granite field wall, 1.1 m tall and 0.7 m wide at the foot, with
an upright cope (B2, owner A31; refs: assets-source/refs/walls_hedges ref-1). Prop kind; the pipeline is
`_drystone.build` (numpy-drawn granite stone faces, no Cycles bake). Placed by the runtime on field-boundary
edges (`hex_core::boundary`). Scripted route, headless: make kit-build PIECE=wall_gate."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _drystone  # noqa: E402

_drystone.build("wall_gate", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, kind="gate")
