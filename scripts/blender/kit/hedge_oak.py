"""hedge_oak: the 4 m run a hedgerow oak rises from: the runtime stands tree_oak_b (about 8 m tall) at its
pivot, and the cards clear the trunk below its crown. Untrimmed hawthorn-and-hazel field hedge, 1.9 m tall and
1.5 m wide at the foot (B2, owner A31; refs: assets-source/refs/walls_hedges ref-2). Prop kind; the pipeline
is `_hedge.build` (numpy leaf-cluster cards in the matte verge-bush green, no Cycles bake). Placed by the
runtime on field-boundary edges (`hex_core::boundary`). Scripted route, headless: make kit-build
PIECE=hedge_oak."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _hedge  # noqa: E402

# Card rows over the profile, spacing along the arm (m), card size (m), outline raggedness (m at the
# top), sprigs per arm, inner cards per metre and the card tilt off the surface normal.
SHAPE = dict(rows=13, spacing=0.28, card=0.46, ragged=0.14, sprigs=8, inner_per_m=12, tilt=0.55)
# Per LOD: every n-th outer card, card grid, card scale, every n-th inner card, core profile sides.
LODS = (dict(every=1, grid=1, scale=1.0, inner=1, core_sides=8),
        dict(every=2, grid=1, scale=1.35, inner=3, core_sides=4),
        dict(every=8, grid=1, scale=2.3, inner=0, core_sides=4))
_hedge.build("hedge_oak", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, kind="run", shape=SHAPE,
             lods=LODS, trunk_clear=0.32)
