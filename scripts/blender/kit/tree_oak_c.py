"""tree_oak_c: an old, broad, low oak, about 7 m tall with a crown about 9.7 m wide (crown width /
height about 1.4 against tree_oak's 1.0 and tree_oak_b's 0.85), the third variant of the oak tile's
scatter oak (refs: assets-source/refs/forest). Same bark set, leaf tiles, canopy master and palette as
tree_oak; the pipeline is `_oak_scatter.build`. Scripted route, headless: make kit-build PIECE=tree_oak_c.

A short, stout trunk splits low (about 2 m) into seven heavy limbs that run out nearly level before
rising into a wide skirt of low domes, so the clear trunk is about 2.5 m and the limbs show under the
crown; five middle domes and one central top dome make a flat, spreading top. The crown reach and the
trunk's ground reach stay inside tree_oak's (4.86 m and 0.59 m), so the oak tile's scatter spacing,
which takes the largest of the group, is unchanged."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _oak_scatter  # noqa: E402

# Crown tiers (count, ring radius m, centre z m, clump radius m), as `_tree.TIERS`. One top dome on the
# axis: the trunk runs up into it, so it stays upright and its collider inside 0.59 m.
TIERS = ((7, 2.8, 3.8, 1.3), (5, 1.7, 4.8, 1.3), (1, 0.0, 5.6, 1.3))
SHAPE = dict(trunk_r0=0.54, trunk_r1=0.22, flare=0.04, roots=6, root_reach=(1.1, 1.6), root_z0=(0.5, 0.9),
             root_r=(0.16, 0.24), limb_z=(1.6, 2.2), limb_r=(0.27, 0.1), sec_r=(0.12, 0.05), leader_z=3.0,
             leader_r=(0.16, 0.07), wobble=0.12, trunk_step=0.55, limb_step=0.8, sec_step=0.7)
_oak_scatter.build("tree_oak_c", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, tiers=TIERS,
                   shape=SHAPE)
