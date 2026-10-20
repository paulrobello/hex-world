"""shrub_forest_c: the pine forest's third undergrowth shrub, a bramble mound, about 1 m tall and 2.2 m
across: a low, wide, lumpy mound of fresh yellow-green leaves with reddish arching canes that climb out
of its top and bow down past its rim (refs: assets-source/refs/forest, ref-1's "undergrowth of ferns,
shrubs and saplings"). Prop kind (LOD0 <= 1,500 tris); the pipeline is `_shrub_bramble.build` over
`_shrub.build` (numpy leaf-cluster cards, colour + masks canes, no Cycles bake).
Scripted route, headless: make kit-build PIECE=shrub_forest_c."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _shrub_bramble  # noqa: E402

# sRGB: a fresh yellow-green, lighter and warmer than the hazel (#48742a) so the mound reads in spruce
# shade (owner Q60: the holly went near black there); reddish bramble twigs in the leaf tiles.
PALETTE = dict(DARK="#2d4a18", MID="#5a8a2e", LIGHT="#a0c05a", BACK="#283f1a", TWIG="#6a3a32")
# Crown: the family ellipsoid (centre (cx, 0, cz), semi-axes a across and b up), flattened into a low
# mound and made lumpy by `_shrub_bramble`: `lobes` is the radius lobe amplitude, `hump` the height
# one. `spread` 1.35 runs the outer cards' Fibonacci rows down to about zf -0.5, near the litter.
CROWN = dict(cx=0.0, cz=0.38, a=0.8, b=0.48, outer=96, inner=20, card=0.46, cushion=0.05, spread=1.35,
             zmin=-0.6, lobes=0.15, hump=0.2, bumps=7)
# Canes: each crests near `apex` m (about the mound's top) and hangs its tip `tip_z` m above the
# litter at `reach` m, past the leaf rim. sink 0.08 keeps the base ring below the litter.
STEM = dict(count=8, apex=0.96, reach=1.06, tip_z=0.26, r0=0.017, r1=0.008, sink=0.08)
# Per LOD: every n-th outer card, card grid (2: cushioned 8 tris, 1: flat 2), card scale, every n-th
# inner card (0: none), canes kept, cane sides, skeleton stride (13 points: 2, 4 and 6 end on the tip).
LODS = (dict(every=1, grid=2, scale=1.0, inner=1, stems=8, sides=3, stride=2),
        dict(every=1, grid=1, scale=1.06, inner=1, stems=8, sides=3, stride=4),
        dict(every=2, grid=1, scale=1.32, inner=3, stems=4, sides=3, stride=6))
_shrub_bramble.build("shrub_forest_c", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
                     palette=PALETTE, crown=CROWN, stem=STEM, lods=LODS, height=(0.85, 1.25),
                     reach_range=(0.95, 1.3), core_r=0.69, core_h=0.8)
