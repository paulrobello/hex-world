"""wildflower_single_c: one blue cornflower on its own stem, about 0.95 m, the clumps' cornflower
(refs: assets-source/refs/meadow_props). The pipeline is `_single.build`. Scripted route, prop
kind, numpy-drawn tiles, no Cycles bake: make kit-build PIECE=wildflower_single_c."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _single  # noqa: E402

_single.build("wildflower_single_c", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
              species="cornflower", height=0.95)
