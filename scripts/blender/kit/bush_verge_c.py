"""bush_verge_c: the third road-verge bush, a flowering hawthorn-like bush about 1.05 m tall and 1.0 m
across, upright with an uneven sprayed outline, a deep fresh green mass of small leaves dotted with clusters
of cream-white blossom (refs: assets-source/refs/road_straight, ref-1's "grass verges with tufts and
small bushes"). Prop kind (LOD0 <= 1,500 tris); the pipeline is `_verge_bush_c.build` over
`_verge_bush.build` and `_shrub.build` (numpy leaf-cluster cards with painted blossom, colour + masks
stems, no Cycles bake). Scripted route, headless: make kit-build PIECE=bush_verge_c."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _verge_bush_c  # noqa: E402

# sRGB: a deep fresh hawthorn green, between bush_verge's box-green and bush_verge_b's yellow-green,
# dark enough that the cream blossom stands out.
PALETTE = dict(DARK="#1b3613", MID="#365f22", LIGHT="#6f9842", BACK="#10220b", TWIG="#4a3a2a")
# Crown: the family ellipsoid (centre (cx, 0, cz), semi-axes a across and b up), upright (b > a);
# `spread` ~1.05 runs the outer cards' Fibonacci rows down to zmin, near the grass. `_verge_bush_c`'s
# sprays: `sprays` seeded directions over the upper two thirds push nearby cards out by up to
# `spray` of their radius, over `spray_width` radians.
CROWN = dict(cx=0.0, cz=0.52, a=0.4, b=0.5, outer=112, inner=30, card=0.28, cushion=0.04, spread=1.05,
             zmin=-0.92, sprays=5, spray=0.26, spray_width=0.45)
# sink 0.1 is below the first kept ring's climb (length / 12 x stride x sin(el0) <= 0.075 m).
STEM = dict(count=3, length=0.3, r0=0.018, r1=0.008, sink=0.1)
# Per LOD: every n-th outer card, card grid (2: cushioned 8 tris, 1: flat 2), card scale, every n-th
# inner card (0: none), stems kept, stem sides, skeleton stride.
LODS = (dict(every=1, grid=2, scale=1.0, inner=1, stems=3, sides=5, stride=3),
        dict(every=1, grid=1, scale=1.05, inner=1, stems=3, sides=4, stride=6),
        dict(every=2, grid=1, scale=1.3, inner=3, stems=1, sides=3, stride=12))
# Blossom corymbs on outer tiles 0, 1 and 2 (tile 3 is the leaf-only inner tile).
_verge_bush_c.build("bush_verge_c", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
                    clusters=(7, 6, 5), palette=PALETTE, crown=CROWN, stem=STEM, lods=LODS,
                    height=(1.0, 1.25), reach_range=(0.45, 0.75), core_r=0.31, core_h=0.75)
