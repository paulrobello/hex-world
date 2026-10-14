"""house_cottage: a one-and-a-half-storey timber-framed English cottage (owner-approved style,
assets-source/refs/house_cluster ref-1 left house and ref-2): cream lime-rendered panels between dark
oak framing, a steep clay-tile roof, a stone stack on the right gable with two pots, oak casements
and a plank door. Scripted route; the shared builder and its contracts are in _house.py.

Footprint 4.4 m deep (x) by 6.4 m wide (y), eaves at 2.6 m, 47° roof: real-metre scale with the
slightly exaggerated roof and stack of the diorama style. The front door faces +x (yaw 0)."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _house  # noqa: E402

P = "house_cottage"
WALLS, ROOF, STONE, TIMBER, FRAME, DOOR, GLASS = (f"{P}_{n}" for n in ("walls", "roof", "stone", "timber", "frame", "door", "glass"))
L = _house.PLASTER_LIN
SPEC = {
    "piece": P,
    "depth": 4.4, "width": 6.4, "eave": 2.6, "pitch": 47.0,
    "overhang": 0.3, "gable_overhang": 0.25, "courses": 11, "kick": 0.04, "roof_thick": 0.14,
    "chimney": (-0.3, 1), "chimney_above": 0.75,
    # Blender colours (linear). Tileable slots show manifest colour x plaster albedo (PLASTER_LIN).
    "materials": {
        WALLS: ((1.0 * L, 0.95 * L, 0.84 * L), 0.9),
        ROOF: ((0.50 * L, 0.19 * L, 0.095 * L), 0.85),
        STONE: ((0.42 * L, 0.39 * L, 0.34 * L), 0.9),
        TIMBER: ((0.075, 0.05, 0.035), 0.8),
        FRAME: ((0.16, 0.10, 0.055), 0.75),
        DOOR: ((0.20, 0.12, 0.06), 0.7),
        GLASS: ((0.20, 0.16, 0.10), 0.2),
    },
    # masks hooks per slot: (snow capacity, porosity, night emissive).
    "hooks": {WALLS: (1.0, 0.5, 0.0), ROOF: (1.0, 0.35, 0.0), STONE: (1.0, 0.4, 0.0), TIMBER: (1.0, 0.6, 0.0),
              FRAME: (1.0, 0.5, 0.0), DOOR: (1.0, 0.5, 0.0), GLASS: (0.0, 0.05, 1.0)},
    "slots": {"walls": WALLS, "roof": ROOF, "stone": STONE, "timber": TIMBER, "frame": FRAME, "door": DOOR,
              "glass": GLASS, "sill": STONE, "pots": ROOF},
    # (wall, u, sill z, width, height). Wall u runs along +y on the front, -y on the back, +x on the
    # left gable (-y) and -x on the right gable (+y).
    "windows": [("front", -1.8, 0.95, 0.9, 1.0), ("front", 1.75, 0.95, 0.9, 1.0),
                ("back", -1.6, 0.95, 0.9, 1.0), ("back", 1.6, 0.95, 0.9, 1.0),
                ("left", 0.0, 2.85, 0.6, 0.7), ("right", -1.2, 1.0, 0.7, 0.9)],
    "door": (0.1, 0.9, 2.05),
    # Timber framing: (wall, u0, z0, u1, z1, minor). Minor members drop at LOD1.
    "timbers": [
        ("front", -3.2, 0.42, 3.2, 0.42, False), ("front", -3.2, 2.5, 3.2, 2.5, False),
        ("front", -3.13, 0.42, -3.13, 2.55, False), ("front", 3.13, 0.42, 3.13, 2.55, False),
        ("front", -0.53, 0.42, -0.53, 2.5, True), ("front", 0.73, 0.42, 0.73, 2.5, True),
        ("front", -2.5, 0.42, -2.5, 2.5, True), ("front", 2.45, 0.42, 2.45, 2.5, True),
        ("front", -3.1, 1.3, -2.5, 2.45, True), ("front", 3.1, 1.3, 2.45, 2.45, True),
        ("front", -3.1, 0.82, -0.55, 0.82, True), ("front", 0.75, 0.82, 3.1, 0.82, True),
        ("back", -3.2, 0.42, 3.2, 0.42, False), ("back", -3.2, 2.5, 3.2, 2.5, False),
        ("back", -3.13, 0.42, -3.13, 2.55, False), ("back", 3.13, 0.42, 3.13, 2.55, False),
        ("back", 0.0, 0.42, 0.0, 2.5, True), ("back", -2.4, 0.42, -2.4, 2.5, True), ("back", 2.4, 0.42, 2.4, 2.5, True),
        ("back", -0.1, 1.2, -0.95, 2.45, True), ("back", 0.1, 1.2, 0.95, 2.45, True),
        ("back", -3.1, 0.82, 3.1, 0.82, True),
        ("left", -2.2, 0.42, 2.2, 0.42, False), ("left", -2.2, 2.5, 2.2, 2.5, False),
        ("left", -2.13, 0.42, -2.13, 2.55, False), ("left", 2.13, 0.42, 2.13, 2.55, False),
        ("left", -0.6, 2.55, -0.6, 4.2, True), ("left", 0.6, 2.55, 0.6, 4.2, True),
        ("left", -2.05, 2.55, -0.65, 3.9, True), ("left", 2.05, 2.55, 0.65, 3.9, True),
        ("left", 0.0, 0.42, 0.0, 2.5, True), ("left", -2.1, 1.2, -0.1, 2.45, True), ("left", 2.1, 1.2, 0.1, 2.45, True),
        ("right", -2.2, 0.42, 2.2, 0.42, False), ("right", -2.2, 2.5, 2.2, 2.5, False),
        ("right", -2.13, 0.42, -2.13, 2.55, False), ("right", 2.13, 0.42, 2.13, 2.55, False),
        ("right", -2.05, 2.55, -0.9, 3.9, True), ("right", -2.1, 0.82, -0.45, 0.82, True),
    ],
}

_house.build_piece(SPEC, __file__)
