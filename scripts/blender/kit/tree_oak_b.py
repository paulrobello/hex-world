"""tree_oak_b: a younger, narrower oak, about 8 m tall with a crown about 5.5 m wide (crown width /
height about 0.7 against tree_oak's 1.0), the variant of the forest tiles' scatter oak (refs:
assets-source/refs/forest). Same bark set, leaf tiles, canopy master and palette as tree_oak; the
pipeline is `_oak_scatter.build`. Scripted route, headless: make kit-build PIECE=tree_oak_b.

Ten smaller clumps in three tiers (4 low, 4 middle, 2 top) set close to a slimmer trunk that runs
on as the leader; the low limbs leave it higher, so the clear trunk is about 3.5 m."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _oak_scatter  # noqa: E402

# Crown tiers (count, ring radius m, centre z m, clump radius m), as `_tree.TIERS`.
TIERS = ((4, 1.6, 4.4, 1.15), (4, 1.0, 5.7, 1.15), (2, 0.45, 6.9, 1.1))
SHAPE = dict(trunk_r0=0.22, trunk_r1=0.09, flare=0.12, roots=4, root_reach=(0.6, 0.9), root_z0=(0.3, 0.55),
             root_r=(0.09, 0.13), limb_z=(2.8, 3.4), limb_r=(0.12, 0.05), sec_r=(0.07, 0.035), leader_z=4.0,
             leader_r=(0.1, 0.05), wobble=0.07, trunk_step=0.6, limb_step=0.8, sec_step=0.7)
_oak_scatter.build("tree_oak_b", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, tiers=TIERS,
                   shape=SHAPE)
