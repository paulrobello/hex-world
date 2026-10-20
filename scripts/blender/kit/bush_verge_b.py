"""bush_verge_b: the second road-verge bush, smaller and looser than bush_verge, about 0.65 m tall and
0.9 m across, a light yellow-green mass of small leaves with an uneven, slightly leaning outline
(refs: assets-source/refs/road_straight, ref-1's "grass verges with tufts and small bushes": the
small pale bush beside the dark round one). Prop kind (LOD0 <= 1,500 tris); the pipeline is
`_verge_bush_b.build` over `_verge_bush.build` and `_shrub.build` (numpy leaf-cluster cards,
colour + masks stems, no Cycles bake). Scripted route, headless: make kit-build PIECE=bush_verge_b."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _verge_bush_b  # noqa: E402

# sRGB: a fresh yellow-green, lighter than bush_verge's box-green and yellower than the hazel.
PALETTE = dict(DARK="#2f4a14", MID="#5c8526", LIGHT="#a6c458", BACK="#1d2f0c", TWIG="#5a4a30")
# Crown: as bush_verge (`spread` ~1.05 runs the outer cards down to zmin, near the grass), smaller,
# with fewer cards for its area and a thinner inner fill (looser), plus `_verge_bush_b`'s lumps:
# `lump` radial wave amplitude, `oval` plan y scale, `lean` x shift per metre above the centre.
CROWN = dict(cx=0.0, cz=0.3, a=0.38, b=0.3, outer=84, inner=18, card=0.24, cushion=0.035, spread=1.05,
             zmin=-0.92, lump=0.22, oval=0.98, lean=0.14)
# sink 0.1: deeper than the first kept ring's climb (length / 12 x stride x sin(el0) ~ 0.045 m), so
# no ring straddles z = 0 with wind U > 0 below ground (bush_verge friction 2).
STEM = dict(count=3, length=0.18, r0=0.015, r1=0.007, sink=0.1)
LODS = (dict(every=1, grid=2, scale=1.0, inner=1, stems=3, sides=5, stride=3),
        dict(every=1, grid=1, scale=1.05, inner=1, stems=3, sides=4, stride=6),
        dict(every=2, grid=1, scale=1.3, inner=3, stems=1, sides=3, stride=12))
_verge_bush_b.build("bush_verge_b", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, palette=PALETTE,
                    crown=CROWN, stem=STEM, lods=LODS, height=(0.6, 0.72), reach_range=(0.4, 0.58), core_r=0.29,
                    core_h=0.5)
