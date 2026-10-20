"""shrub_forest_b: the pine forest's second undergrowth shrub, holly-like, about 1.8 m tall and 1.4 m
across: a dense dark, glossy blue-green evergreen cone, broadest low down and closing to a rounded
point, with a few red berry clusters on its outer leaves (refs: assets-source/refs/forest, ref-1's
"undergrowth of ferns, shrubs and saplings"). Prop kind (LOD0 <= 1,500 tris); the pipeline is
`_shrub_holly.build` over `_shrub.build` (numpy leaf-cluster cards, colour + masks stems, no Cycles
bake). Scripted route, headless: make kit-build PIECE=shrub_forest_b."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _shrub_holly  # noqa: E402

# sRGB: a deep blue-green holly, cooler than the spruce needles and the box-green verge bushes.
PALETTE = dict(DARK="#173323", MID="#28573a", LIGHT="#64965e", BACK="#0f2015", TWIG="#4a4336")
# Crown: the family ellipsoid (centre (cx, 0, cz), semi-axes a across and b up) reshaped into an
# ogive by `_shrub_holly`: `flare` is the radius fraction at the bottom, `ogive` the top's closing
# exponent. `spread` ~1.05 runs the outer cards' Fibonacci rows down to zmin, near the litter.
CROWN = dict(cx=0.0, cz=0.9, a=0.68, b=0.82, outer=110, inner=26, card=0.5, cushion=0.05, spread=1.05,
             zmin=-0.93, flare=0.78, ogive=0.55)
# sink 0.08 is below the first kept ring's climb (length / 12 x stride x sin(el0) >= 0.12 m at stride
# 3), so no stem ring straddles z = 0 with wind U > 0 below ground (bush_verge friction 2).
STEM = dict(count=5, length=0.7, r0=0.024, r1=0.009, sink=0.08)
# Per LOD: every n-th outer card, card grid (2: cushioned 8 tris, 1: flat 2), card scale, every n-th
# inner card (0: none), stems kept, stem sides, skeleton stride.
LODS = (dict(every=1, grid=2, scale=1.0, inner=1, stems=5, sides=5, stride=3),
        dict(every=1, grid=1, scale=1.06, inner=1, stems=5, sides=4, stride=6),
        dict(every=2, grid=1, scale=1.32, inner=3, stems=3, sides=3, stride=12))
_shrub_holly.build("shrub_forest_b", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
                   palette=PALETTE, berries=5, crown=CROWN, stem=STEM, lods=LODS, height=(1.0, 2.6),
                   reach_range=(0.4, 1.5), core_r=0.55, core_h=1.2)
