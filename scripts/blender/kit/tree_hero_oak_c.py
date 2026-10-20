"""tree_hero_oak_c: the oak tile's third hero oak (PRD §6 Variety), an ancient stag-headed oak: the hero's
upright twisted bole under a low, broad crown broken into four lobes, with a dead top. Same bark set,
canopy master, leaf tiles, palette, canopy gaps, wind data and 1.2x scale as the hero; the pipeline is
`_oak_ancient.build`. Scripted route, headless: make kit-build PIECE=tree_hero_oak_c.
Refs: assets-source/refs/tree_hero_oak (ref-1).

Four lobes leave a wide gap to the south-east and notches between them: the east, north and west lobes
are each a low outer dome on a long low limb and a higher inner dome on a secondary, the south-west lobe
one low dome. The living crown tops out about 2.9 m under the hero's (10.4 m at 1.2x). Two bare dead leaders rise from the bole through the crown's middle and
stand clear above it with short dead tines (the stag's antlers), still under the hero's height, and a
broken dead limb stub juts into the south-east gap."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _oak_ancient  # noqa: E402

# (x, y, z, radius, feed) at 1x, Blender frame (+y north, -y south: the default camera's side).
CLUMPS = [
    (4.357, 0.612, 5.0, 1.9, "limb"),     # east lobe, outer
    (2.685, 1.550, 6.9, 1.65, "sec"),     # east lobe, inner
    (-0.729, 4.136, 5.6, 1.85, "limb"),   # north lobe
    (-1.721, 2.457, 7.2, 1.6, "sec"),
    (-4.400, 0.000, 4.7, 1.9, "heavy"),   # west lobe
    (-2.913, -1.060, 6.5, 1.6, "sec"),
    (-2.279, -3.647, 4.2, 1.85, "heavy"),  # south-west lobe: one low dome, the lowest
]
# Dead wood (1x): two antler leaders from inside the bole top, rising through the open middle of the
# crown and spreading above it, each with tines (t, direction, length, r0, r1), and one broken stub into
# the south-east gap (a blunt end: it keeps most of its radius).
DEAD = {
    "leaders": [
        ((0.10, -0.08, 3.25), (1.5, -1.3, 10.4), 0.32, 0.07,
         [(0.55, (1.0, -0.2, 0.7), 2.0, 0.11, 0.04), (0.78, (-0.5, -1.0, 0.9), 1.6, 0.09, 0.035)]),
        ((-0.08, 0.10, 3.3), (-1.5, 0.9, 9.9), 0.29, 0.07,
         [(0.62, (-1.0, -0.4, 0.8), 1.8, 0.10, 0.04), (0.84, (0.4, 1.0, 1.0), 1.4, 0.08, 0.03)]),
    ],
    "stubs": [((0.22, -0.22, 2.5), (2.55, -2.55, 3.35), 0.36, 0.22)],
}
SHAPE = dict(lean=(0.0, 0.0), fork_z=3.3, leader_a_r=(0.42, 0.14), leader_b_r=(0.4, 0.13), limb_z=(2.6, 3.0),
             limb_r=(0.5, 0.17), heavy_z=2.3, heavy_r=(0.55, 0.2), heavy_ease=2.6, sec_r=(0.21, 0.08), dead=DEAD)
_oak_ancient.build("tree_hero_oak_c", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, clumps=CLUMPS,
                   shape=SHAPE)
