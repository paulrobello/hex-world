"""shrub_forest: a low rounded woodland shrub, hazel-like, about 1.4 m tall and 1.9 m across: a dense
dome of small ovate leaves on seven woody stems (refs: assets-source/refs/forest, ref-1's "undergrowth
of ferns, shrubs and saplings"). Prop kind (LOD0 <= 1,500 tris); the pipeline is `_shrub.build`
(numpy leaf-cluster cards, colour + masks stems, no Cycles bake).
Scripted route, headless: make kit-build PIECE=shrub_forest."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _shrub  # noqa: E402

# Crown: an ellipsoid centred at (cx, 0, cz) with semi-axes a (across) and b (up); `outer` cards on
# its shell, `inner` inside; cards `card` m across with a `cushion` m centre lift.
CROWN = dict(cx=0.0, cz=0.64, a=0.8, b=0.64, outer=92, inner=28, card=0.5, cushion=0.06, spread=1.95, zmin=-0.9)
STEM = dict(count=7, length=0.8, r0=0.022, r1=0.008, sink=0.08)
# Per LOD: every n-th outer card, card grid (2: cushioned 8 tris, 1: flat 2), card scale, every n-th
# inner card (0: none), stems kept, stem sides, skeleton stride.
LODS = (dict(every=1, grid=2, scale=1.0, inner=1, stems=7, sides=5, stride=3),
        dict(every=1, grid=1, scale=1.06, inner=1, stems=7, sides=4, stride=6),
        dict(every=2, grid=1, scale=1.32, inner=3, stems=3, sides=3, stride=12))
_shrub.build("shrub_forest", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, crown=CROWN, stem=STEM,
             lods=LODS, height=(1.2, 1.65), reach_range=(0.8, 1.15), core_r=0.7, core_h=1.0)
