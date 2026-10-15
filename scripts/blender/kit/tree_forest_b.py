"""tree_forest_b: a tall, slender Norway spruce, about 20.5 m with a narrow crown starting high on a
bare trunk (crown width / height about 0.27 against tree_forest's 0.38). Same species, bark set and
needle tiles as tree_forest (refs: assets-source/refs/forest); the pipeline is `_spruce.build`.
Scripted route, headless: make kit-build PIECE=tree_forest_b.

The 20.7 m trunk island needs uvPerM 1/80 to fit below NEEDLE_V0, so the manifest bark slot's
tileUvScale is 1 / (uvPerM x 2 m) = 40 (bark stays at 512 px/m)."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _spruce  # noqa: E402

LODS = (dict(sides=6, step=1.4, core=(8, 6), segs=2, folded=True, fsegs=2, ffolded=True, every=1, width=1.0, leader=2),
        dict(sides=5, step=2.8, core=(6, 3), segs=2, folded=False, fsegs=1, ffolded=True, every=1, width=1.0, leader=2),
        dict(sides=4, step=5.5, core=(6, 2), segs=2, folded=False, fsegs=None, ffolded=False, every=2, width=1.3,
             leader=1))
_spruce.build("tree_forest_b", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
              shape=dict(HEIGHT=20.5, Z0=3.6, ZTOP=19.4, WHORLS=26, RMAX=2.5, TRUNK_R0=0.3, TRUNK_R1=0.035),
              flare=0.17, bark_uv_per_m=1 / 80, lods=LODS)
