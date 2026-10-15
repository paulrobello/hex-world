"""wildflower_clump_b: a ~1 m clump of meadow grass with white daisies and yellow buttercups (the
yellow/white mix), the same clump shape, height range and grass as wildflower_clump (refs:
assets-source/refs/meadow_props). The pipeline is `_flowers.build`. Scripted route, prop kind
(LOD0 <= 1,500 tris), numpy-drawn tiles, no Cycles bake: make kit-build PIECE=wildflower_clump_b."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _flowers  # noqa: E402

_flowers.build("wildflower_clump_b", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
               species=("daisy", "buttercup"))
