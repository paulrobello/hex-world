"""sapling_spruce_c: a third young Norway spruce of the forest undergrowth (refs: assets-source/refs/
forest, ref-1: "undergrowth of ferns, shrubs and saplings"), shade-suppressed: a 2.0 m stem (leader tip
2.25 m), bare to 1.1 m and crooked 0.14 m to one side through the crown, five sparse, level tiers of long
narrow fronds (every other branch of a whorl), nearly as wide at the top as at the bottom (0.6-0.75 m),
a thin leader and a thin dark heart. Same bark set, needle tiles and wind data as sapling_spruce and sapling_spruce_b; the pipeline
is `_spruce.build` through `_sapling`, `_sapling_b` and `_sapling_c`. Scripted route, headless:
make kit-build PIECE=sapling_spruce_c. Kind `tree`.

Within the sapling group's limits, asserted in `_sapling_c.build`: collider reach <= 0.1 m and crown
reach <= 1.1 m (the group's scatter footprint), top below sapling_spruce's 2.45 m; and the checks
`_spruce.build` and `_sapling_b.build` make (every needle card above 0.6 m, wind U <= 0.1 below 0.5 m)."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _sapling_c  # noqa: E402

LODS = (dict(sides=6, step=0.4, core=(6, 3), segs=4, folded=True, fsegs=2, ffolded=True, every=1, width=0.8, leader=2),
        dict(sides=5, step=1.0, core=(5, 2), segs=2, folded=False, fsegs=1, ffolded=True, every=1, width=0.8, leader=2),
        dict(sides=4, step=2.5, core=(3, 1), segs=2, folded=False, fsegs=None, ffolded=False, every=3, width=1.5,
             leader=1))
_sapling_c.build("sapling_spruce_c", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
                 shape=dict(HEIGHT=2.0, Z0=1.1, ZTOP=1.92, WHORLS=5, RMAX=0.15, TRUNK_R0=0.03, TRUNK_R1=0.009),
                 tip_floor=0.6, leader_scale=0.35, droop_scale=0.8, flare=0.01, bark_uv_per_m=1 / 64, lods=LODS,
                 filler_whorls=1, core=dict(scale=0.22, tip=0.1, foot=0.06), lod0_max=1000)
