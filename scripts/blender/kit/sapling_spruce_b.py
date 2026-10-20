"""sapling_spruce_b: a second young Norway spruce of the forest undergrowth (refs: assets-source/refs/
forest, ref-1: "undergrowth of ferns, shrubs and saplings"), so the saplings are not all identical:
about 1.7 m, shorter, wider and bushier than sapling_spruce's 2.2 m tiered whip, six close whorls of
level branches with a filler frond on every whorl, a fuller dark heart, and a leader bent to one
side. Same bark set, needle tiles and wind data; the pipeline is `_spruce.build` through `_sapling`
and `_sapling_b`. Scripted route, headless: make kit-build PIECE=sapling_spruce_b. Kind `tree`.

The lowest whorl sits at 0.92 m so its widest, folded cards stay above 0.6 m (`_spruce.build`'s
assert; the gate's still band is below 0.5 m)."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _sapling_b  # noqa: E402

LODS = (dict(sides=6, step=0.5, core=(6, 3), segs=3, folded=True, fsegs=2, ffolded=True, every=1, width=1.0, leader=2),
        dict(sides=5, step=1.0, core=(5, 2), segs=2, folded=False, fsegs=1, ffolded=True, every=1, width=1.0, leader=2),
        dict(sides=4, step=2.5, core=(4, 1), segs=2, folded=False, fsegs=None, ffolded=False, every=3, width=1.5,
             leader=1))
_sapling_b.build("sapling_spruce_b", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
                 shape=dict(HEIGHT=1.7, Z0=0.92, ZTOP=1.55, WHORLS=6, RMAX=0.8, TRUNK_R0=0.032, TRUNK_R1=0.01),
                 flare=0.012, bark_uv_per_m=1 / 64, lods=LODS, filler_whorls=6,
                 core=dict(scale=0.5, tip=0.15, foot=0.2), lod0_max=1000)
