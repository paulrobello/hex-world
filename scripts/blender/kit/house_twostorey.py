"""house_twostorey: a two-storey rendered farmhouse (assets-source/refs/house_cluster ref-1 right house):
cream lime render with dressed stone quoins, a grey slate roof, a stone stack on the left gable,
white-painted sash windows and a small slate-roofed porch over a painted door. Scripted route; the
shared builder and its contracts are in _house.py.

Footprint 5.0 m deep (x) by 7.2 m wide (y), eaves at 5.0 m, 40° roof. The front door faces +x
(yaw 0): on house_cluster it closes the far end of the yard, facing the driveway."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _house  # noqa: E402

P = "house_twostorey"
WALLS, ROOF, STONE, FRAME, DOOR, GLASS = (f"{P}_{n}" for n in ("walls", "roof", "stone", "frame", "door", "glass"))
L = _house.PLASTER_LIN
SPEC = {
    "piece": P,
    "depth": 5.0, "width": 7.2, "eave": 5.0, "pitch": 40.0,
    "overhang": 0.3, "gable_overhang": 0.2, "courses": 13, "kick": 0.025, "roof_thick": 0.14,
    "chimney": (0.0, -1), "chimney_above": 0.8,
    "bars": False,
    # Blender colours (linear). Tileable slots show manifest colour x plaster albedo (PLASTER_LIN).
    "materials": {
        WALLS: ((0.98 * L, 0.94 * L, 0.86 * L), 0.9),
        ROOF: ((0.105 * L, 0.11 * L, 0.125 * L), 0.7),
        STONE: ((0.48 * L, 0.45 * L, 0.40 * L), 0.9),
        FRAME: ((0.70, 0.69, 0.65), 0.6),
        DOOR: ((0.10, 0.16, 0.12), 0.5),
        GLASS: ((0.20, 0.16, 0.10), 0.2),
    },
    # masks hooks per slot: (snow capacity, porosity, night emissive).
    "hooks": {WALLS: (1.0, 0.5, 0.0), ROOF: (1.0, 0.2, 0.0), STONE: (1.0, 0.4, 0.0),
              FRAME: (1.0, 0.3, 0.0), DOOR: (1.0, 0.3, 0.0), GLASS: (0.0, 0.05, 1.0)},
    "slots": {"walls": WALLS, "roof": ROOF, "stone": STONE, "frame": FRAME, "door": DOOR,
              "glass": GLASS, "sill": STONE, "pots": STONE},
    # (wall, u, sill z, width, height). Wall u runs along +y on the front, -y on the back, +x on the
    # left gable (-y) and -x on the right gable (+y).
    "windows": [("front", -2.2, 0.95, 1.0, 1.3), ("front", 2.2, 0.95, 1.0, 1.3),
                ("front", -2.2, 3.15, 1.0, 1.2), ("front", 0.0, 3.25, 0.8, 1.05), ("front", 2.2, 3.15, 1.0, 1.2),
                ("back", -2.0, 0.95, 1.0, 1.3), ("back", 2.0, 0.95, 1.0, 1.3),
                ("back", -2.0, 3.15, 1.0, 1.2), ("back", 2.0, 3.15, 1.0, 1.2),
                ("left", 1.55, 0.95, 0.8, 1.2), ("left", 1.55, 3.15, 0.8, 1.1),
                ("right", 0.0, 0.95, 0.9, 1.2), ("right", 0.0, 3.15, 0.9, 1.2)],
    "door": (0.0, 1.0, 2.2),
    # Quoins: (count per corner, block height, vertical step).
    "quoins": (7, 0.4, 0.66),
    # Porch: (u, width, depth out from the wall, eave height).
    "porch": (0.0, 1.9, 1.1, 2.75),
}

_house.build_piece(SPEC, __file__)
