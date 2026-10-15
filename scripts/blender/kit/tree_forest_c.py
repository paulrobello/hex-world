"""tree_forest_c: a young, short Norway spruce, about 8 m, a broad full cone down to about 1 m
over a thin trunk (crown width / height about 0.6 against tree_forest's 0.38). Same species, bark
set and needle tiles as tree_forest (refs: assets-source/refs/forest); the pipeline is
`_spruce.build`. Scripted route, headless: make kit-build PIECE=tree_forest_c.

The lowest whorl sits at 1.55 m so the drooping, folded cards stay above 0.6 m (the gate's still
band is below 0.5 m). The short trunk and core are a larger share of a small tree, so LOD2 keeps
every third main card (scaled up 1.5x to hold coverage) and a 5-sided, one-ring core."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _spruce  # noqa: E402

LODS = (dict(sides=6, step=1.0, core=(8, 5), segs=2, folded=True, fsegs=2, ffolded=True, every=1, width=1.0, leader=2),
        dict(sides=5, step=2.0, core=(6, 2), segs=2, folded=False, fsegs=1, ffolded=True, every=1, width=1.0, leader=2),
        dict(sides=4, step=4.5, core=(5, 1), segs=2, folded=False, fsegs=None, ffolded=False, every=3, width=1.5,
             leader=1))
_spruce.build("tree_forest_c", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
              shape=dict(HEIGHT=8.2, Z0=1.55, ZTOP=7.3, WHORLS=15, RMAX=2.25, TRUNK_R0=0.14, TRUNK_R1=0.025),
              flare=0.08, bark_uv_per_m=1 / 64, lods=LODS)
