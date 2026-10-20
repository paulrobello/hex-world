"""tree_oak: a mature broad oak, about 9.5 m tall with a crown about 9.5 m wide, the forest tiles'
scatter oak (refs: assets-source/refs/forest, "spruce among oaks"). The hero oak's look (bark set,
baked leaf-dome canopy, lobed fringe cards, palette, wind data) at scatter size and the `piece`
texture budget; the pipeline is `_oak_scatter.build`. Scripted route, headless:
make kit-build PIECE=tree_oak.

Fourteen clumps in three tiers (6 low, 5 middle, 3 top) on one leaning trunk with five buttress
roots; the clear trunk runs to about 4 m (0.4 of the height, ref-1's oaks)."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _oak_scatter  # noqa: E402

# Crown tiers (count, ring radius m, centre z m, clump radius m), as `_tree.TIERS`.
TIERS = ((6, 2.9, 4.6, 1.35), (5, 1.9, 6.2, 1.35), (3, 0.85, 7.7, 1.3))
SHAPE = dict(trunk_r0=0.36, trunk_r1=0.14, flare=0.22, roots=5, root_reach=(1.0, 1.5), root_z0=(0.45, 0.85),
             root_r=(0.14, 0.22), limb_z=(2.4, 3.2), limb_r=(0.2, 0.08), sec_r=(0.1, 0.05), leader_z=4.2,
             leader_r=(0.16, 0.07), wobble=0.1, trunk_step=0.6, limb_step=0.9, sec_step=0.8)
_oak_scatter.build("tree_oak", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, tiers=TIERS, shape=SHAPE)
