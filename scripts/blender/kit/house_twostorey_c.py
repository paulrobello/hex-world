"""house_twostorey_c: a cross-wing stone-and-render farmhouse, the second house_twostorey variant for
house_cluster's house 2 slot (PRD §6 Variety; owner-approved refs, assets-source/refs/house_cluster
ref-1, its middle house's rubble-stone ground floor and front cross gable): grey-brown rubble stone to
the first floor with dressed quoins under a dressed string course, a pale lime render above, a
weathered stone-slate main roof with a two-storey gabled wing projecting from the middle of the
front, oak bargeboards on the wing gable, a broad stepped rubble stack outside the +y gable with two
clay pots, sage-painted casements and an oxblood plank door under a lean-to stone-slate hood on oak
brackets. Scripted route; the builder and its slot contract are in _house_twostorey_c.py (on
_house.py's and _house_longhouse.py's parts).

Plan (Blender x deep, y wide): the main range 4.4 x 6.4 m (x -2.48..1.92, y ±3.2), eaves 4.6 m, 47°,
ridge 6.96 m; the wing 3.0 m wide (y ±1.5) to x 2.5, eaves 4.85 m, 50°, ridge 6.6 m. The door faces
+x (yaw 0) at y 0 in the wing, so its step reaches house_pad's two-storey doorstep."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _house_twostorey_c  # noqa: E402
from _house import PLASTER_LIN as L  # noqa: E402

P = "house_twostorey_c"
WALLS, RUBBLE, STONE, ROOF, TIMBER, FRAME, DOOR, GLASS, POTS = (
    f"{P}_{n}" for n in ("walls", "rubble", "stone", "roof", "timber", "frame", "door", "glass", "pots"))
SPEC = {
    "piece": P,
    "front": 1.92, "back": -2.48, "half_w": 3.2, "eave": 4.6, "pitch": 47.0,
    "overhang": 0.3, "gable_overhang": 0.2, "courses": 11, "kick": 0.025, "roof_thick": 0.14,
    "wing_front": 2.5, "wing_half": 1.5, "wing_eave": 4.85, "wing_pitch": 50.0,
    "wing_overhang": 0.25, "wing_gable_overhang": 0.2, "wing_courses": 7,
    "stone_top": 2.7,  # the rubble stone's top, under the string course
    "band": (2.63, 2.77, 0.04),  # string course (bottom z, top z, how far proud)
    "apron_top": 0.07,  # flagstones beside the wing, 2 cm over house_pad's gravel (YARD_Z 0.05)
    # Quoins on the rubble storey: (count per corner, block height, vertical step), and the (long,
    # short) block lengths on the main range's corners and on the wing's front corners.
    "quoins": (4, 0.32, 0.6), "quoin_sizes": ((0.36, 0.24), (0.26, 0.18)),
    # The +y gable's stack: centre x, base (x, y, top z), shaft (x, y), shaft top over the main ridge.
    "stack_x": -0.28, "stack_base": (1.4, 0.55, 4.0), "stack_shaft": (0.95, 0.5), "stack_above": 0.7,
    # Lean-to door hood: (top z at the wall, depth out, half width, pitch, slab thickness).
    "hood": (2.55, 0.7, 0.75, 20.0, 0.08),
    # Blender colours (linear). Tileable slots show manifest colour x plaster albedo (PLASTER_LIN).
    "materials": {
        WALLS: ((0.97 * L, 0.93 * L, 0.80 * L), 0.9),
        RUBBLE: ((0.47 * L, 0.43 * L, 0.36 * L), 0.9),
        STONE: ((0.68 * L, 0.63 * L, 0.53 * L), 0.9),
        ROOF: ((0.24 * L, 0.20 * L, 0.16 * L), 0.85),
        TIMBER: ((0.12, 0.075, 0.045), 0.8),
        FRAME: ((0.25, 0.31, 0.23), 0.6),
        DOOR: ((0.26, 0.07, 0.05), 0.6),
        GLASS: ((0.20, 0.16, 0.10), 0.2),
        POTS: ((0.50 * L, 0.19 * L, 0.095 * L), 0.85),
    },
    # masks hooks per slot: (snow capacity, porosity, night emissive).
    "hooks": {WALLS: (1.0, 0.5, 0.0), RUBBLE: (1.0, 0.5, 0.0), STONE: (1.0, 0.4, 0.0), ROOF: (1.0, 0.3, 0.0),
              TIMBER: (1.0, 0.6, 0.0), FRAME: (1.0, 0.4, 0.0), DOOR: (1.0, 0.4, 0.0), GLASS: (0.0, 0.05, 1.0),
              POTS: (1.0, 0.35, 0.0)},
    "slots": {"walls": WALLS, "rubble": RUBBLE, "stone": STONE, "roof": ROOF, "timber": TIMBER, "frame": FRAME,
              "door": DOOR, "glass": GLASS, "sill": STONE, "pots": POTS},
    # (u, width, height) on the wing front, whose u runs along +y.
    "door": (0.0, 1.0, 2.05),
    # Casements: (wall, u, sill z, width, height, major). Walls: F main front and WF wing front (u = y),
    # B back (u = -y), GL left gable (-y, u = x), GR right gable (+y, u = -x). Major windows keep their
    # frame at LOD1 and their sill and bars at LOD0. Each clears the quoins at its corners.
    "windows": [("F", -2.25, 0.85, 1.0, 1.1, True), ("F", 2.25, 0.85, 1.0, 1.1, True),
                ("F", -2.25, 3.2, 0.9, 0.95, True), ("F", 2.25, 3.2, 0.9, 0.95, True),
                ("WF", 0.0, 3.15, 1.1, 1.0, True),
                ("WF", -0.98, 0.95, 0.36, 1.0, False), ("WF", 0.98, 0.95, 0.36, 1.0, False),
                ("WF", 0.0, 5.2, 0.45, 0.5, False),
                ("B", -1.6, 0.9, 0.9, 1.0, False), ("B", 1.6, 0.9, 0.9, 1.0, False),
                ("B", -1.6, 3.2, 0.9, 0.9, False), ("B", 1.6, 3.2, 0.9, 0.9, False),
                ("GL", 0.5, 0.9, 0.8, 1.0, False), ("GL", -0.28, 3.2, 0.8, 0.9, False),
                ("GR", -1.05, 0.9, 0.8, 1.0, False), ("GR", 1.75, 3.2, 0.8, 0.9, False)],
}

_house_twostorey_c.build_piece(SPEC, __file__)
