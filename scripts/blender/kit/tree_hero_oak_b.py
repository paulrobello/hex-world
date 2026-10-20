"""tree_hero_oak_b: the oak tile's second hero oak (PRD §6 Variety), a forked, leaning oak with a lopsided
crown, against tree_hero_oak's round tiered dome. Same bark set, canopy master, leaf tiles, palette,
canopy gaps, wind data and 1.2x scale as the hero; the pipeline is `_oak_fork.build`. Scripted route,
headless: make kit-build PIECE=tree_hero_oak_b. Refs: assets-source/refs/tree_hero_oak (ref-1).

The hero's three twisted strands lean east (+x) and fork at about 3.3 m into two leaders: a tall east
lobe (four domes, top about 13 m at 1.2x) and a lower west lobe (three domes). One heavy limb leaves the
trunk at 2.3 m and runs out nearly level to the south (-y in Blender, +z at runtime) under a low shelf
of two domes. Between the lobes the crown is notched to the north, where the fork shows."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _oak_fork  # noqa: E402

# (x, y, z, radius, feed) at 1x, Blender frame; feed names the branch that runs into the clump.
CLUMPS = [
    (2.8, 0.2, 9.5, 2.0, "leader_a"),   # tall east lobe
    (3.7, 1.7, 7.7, 1.9, "sec"),
    (3.4, -1.4, 7.9, 1.9, "sec"),
    (4.4, 0.1, 5.9, 1.85, "limb"),
    (-3.0, 0.7, 7.3, 1.95, "leader_b"),  # low west lobe, a clear notch between it and the east lobe
    (-3.9, -0.7, 5.9, 1.85, "sec"),
    (-3.0, 2.2, 5.5, 1.8, "limb"),
    (0.4, -4.6, 3.7, 1.85, "heavy"),     # the heavy limb's low south shelf
    (-1.9, -3.9, 4.5, 1.75, "sec"),
    (2.4, -3.4, 5.0, 1.75, "sec"),
]
SHAPE = dict(lean=(1.2, 0.0), fork_z=3.3, leader_a_r=(0.42, 0.14), leader_b_r=(0.4, 0.13), limb_z=(2.6, 3.0),
             limb_r=(0.5, 0.17), heavy_z=2.3, heavy_r=(0.55, 0.2), heavy_ease=2.6, sec_r=(0.21, 0.08))
_oak_fork.build("tree_hero_oak_b", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, clumps=CLUMPS,
                shape=SHAPE)
