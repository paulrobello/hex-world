"""wildflower_single: one white oxeye daisy on its own stem, about 0.85 m so its head clears the
meadow grass, the clumps' daisy (refs: assets-source/refs/meadow_props). The pipeline is
`_single.build`. Scripted route, prop kind, numpy-drawn tiles, no Cycles bake:
make kit-build PIECE=wildflower_single."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _single  # noqa: E402

_single.build("wildflower_single", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
              species="daisy", height=0.85)
