"""bush_verge: a small dense round bush for the road verges, about 1.3 m across and 0.95 m tall, a compact
dark-green mass of small leaves with no stems showing (refs: assets-source/refs/road_straight, ref-1's
"grass verges with tufts and small bushes"). Prop kind (LOD0 <= 1,500 tris); the pipeline is
`_verge_bush.build` over `_shrub.build` (numpy leaf-cluster cards, colour + masks stems, no Cycles bake).
Scripted route, headless: make kit-build PIECE=bush_verge."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _verge_bush  # noqa: E402

# sRGB: a deep box-green with a lighter, slightly yellow sunlit tip (the ref's verge bushes).
PALETTE = dict(DARK="#18300f", MID="#2c5219", LIGHT="#5f8a32", BACK="#0f1f09", TWIG="#3e3224")
# Crown: an ellipsoid centred at (cx, 0, cz) with semi-axes a (across) and b (up); `outer` cards on
# its shell down to zmin (near the ground), `inner` inside; cards `card` m across, `cushion` m lift.
# `spread` ~1.05 runs the outer cards' Fibonacci rows from the top down to zmin (`_shrub.cards`: the
# last card sits at zf = 1 - 2 / spread), so the leaves reach the grass all round.
CROWN = dict(cx=0.0, cz=0.42, a=0.52, b=0.44, outer=130, inner=34, card=0.3, cushion=0.04, spread=1.05, zmin=-0.92)
# sink 0.1: a short stem climbs only ~0.06 m to its 3rd skeleton point, so at a shallower sink that
# ring straddles z = 0 and its lower vertices carry wind U > 0 below ground (`_shrub.build` asserts).
STEM = dict(count=3, length=0.25, r0=0.018, r1=0.008, sink=0.1)
# Per LOD: every n-th outer card, card grid (2: cushioned 8 tris, 1: flat 2), card scale, every n-th
# inner card (0: none), stems kept, stem sides, skeleton stride.
LODS = (dict(every=1, grid=2, scale=1.0, inner=1, stems=3, sides=5, stride=3),
        dict(every=1, grid=1, scale=1.05, inner=1, stems=3, sides=4, stride=6),
        dict(every=2, grid=1, scale=1.3, inner=3, stems=1, sides=3, stride=12))
_verge_bush.build("bush_verge", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, palette=PALETTE,
                  crown=CROWN, stem=STEM, lods=LODS, height=(0.8, 1.2), reach_range=(0.55, 0.8), core_r=0.48,
                  core_h=0.75)
