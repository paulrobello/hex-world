"""wildflower_single_e: one red field poppy on its own hairy stem, about 0.8 m, a scarlet four-petal
cup with a dark centre and one nodding bud, in the single wildflowers' stylised scale (refs:
assets-source/refs/meadow_props). The pipeline is `_single.build`, driven by `_single_poppy`.
Scripted route, prop kind, numpy-drawn tiles, no Cycles bake: make kit-build PIECE=wildflower_single_e."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _single_poppy  # noqa: E402

_single_poppy.build("wildflower_single_e", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
                    height=0.8)
