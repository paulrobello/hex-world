"""sapling_spruce: a young Norway spruce of the forest undergrowth, about 2.2 m, a thin bare stem
under five sparse needle tiers (refs: assets-source/refs/forest, ref-1: "undergrowth of ferns, shrubs
and saplings"). A baby of tree_forest: same bark set, needle tiles, core cone and wind data; the
pipeline is `_spruce.build` through `_sapling` (sapling crown floor, sparse fillers, thin core).
Scripted route, headless: make kit-build PIECE=sapling_spruce. Kind `tree` (wind contract, leaf cards).

The lowest tier sits at 0.85 m so its drooping, folded cards stay above 0.6 m (the gate's still band
is below 0.5 m, where a card's wind U would exceed 0.1)."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _sapling  # noqa: E402

LODS = (dict(sides=6, step=0.5, core=(6, 3), segs=3, folded=True, fsegs=2, ffolded=True, every=1, width=1.0, leader=2),
        dict(sides=5, step=1.0, core=(5, 2), segs=2, folded=False, fsegs=1, ffolded=True, every=1, width=1.0, leader=2),
        dict(sides=4, step=2.5, core=(4, 1), segs=2, folded=False, fsegs=None, ffolded=False, every=3, width=1.5,
             leader=1))
_sapling.build("sapling_spruce", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
               shape=dict(HEIGHT=2.2, Z0=0.85, ZTOP=1.95, WHORLS=5, RMAX=0.62, TRUNK_R0=0.035, TRUNK_R1=0.01),
               flare=0.012, bark_uv_per_m=1 / 64, lods=LODS)
