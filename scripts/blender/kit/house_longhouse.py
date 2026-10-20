"""house_longhouse: a single-storey timber-framed English cottage on an L plan, the third house_cluster
variant (PRD §6 Variety; owner-approved style, assets-source/refs/house_cluster ref-1 and ref-2): a
low hall range with its eaves to the yard and a taller cross wing whose close-studded gable faces the
yard, one front wall plane over both, cream lime render between dark oak framing, brown clay-tile
roofs, a red-brick stack on the hall ridge with two pots, small-paned oak casements and a plank door.
Scripted route; the builder and its contracts are in _house_longhouse.py (on _house.py's parts).

Plan (Blender x deep, y wide; walls inside house_cottage's 4.4 x 6.4 m): the front wall at x = 2.2
from y = -3.2 to 3.2; the hall x -0.8..2.2, y -3.2..0.6 (40° roof, ridge 3.6 m); the wing x
-2.2..2.2, y 0.6..3.2 (55° roof, ridge 4.2 m). Eaves at 2.3 m, 0.3 m under the cottage's. The door
faces +x (yaw 0) at y -0.05, so its step lies on house_pad's doorstep in either cottage slot."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _house_longhouse  # noqa: E402

P = "house_longhouse"
WALLS, ROOF, STONE, BRICK, TIMBER, FRAME, DOOR, GLASS = (
    f"{P}_{n}" for n in ("walls", "roof", "stone", "brick", "timber", "frame", "door", "glass"))
L = _house_longhouse.PLASTER_LIN
SOLE, HPLATE, WPLATE, TIE = 0.40, 2.02, 1.96, 2.22  # sole, hall plate, wing side plate, gable tie beam
SPEC = {
    "piece": P,
    "front": 2.2, "hall_back": -0.8, "wing_back": -2.2, "half_len": 3.2, "wing_y0": 0.6, "eave": 2.3,
    "hall_pitch": 40.0, "hall_overhang": 0.18, "hall_gable_overhang": 0.24, "hall_courses": 8,
    "wing_pitch": 55.0, "wing_overhang": 0.15, "wing_gable_overhang": 0.22, "wing_courses": 8,
    "kick": 0.04, "roof_thick": 0.14,
    "chimney": (0.7, -1.75), "chimney_size": (0.6, 0.95), "chimney_above": 0.95,
    # Blender colours (linear). Tileable slots show manifest colour x plaster albedo (PLASTER_LIN).
    "materials": {
        WALLS: ((1.0 * L, 0.93 * L, 0.78 * L), 0.9),
        ROOF: ((0.36 * L, 0.17 * L, 0.10 * L), 0.85),
        STONE: ((0.45 * L, 0.42 * L, 0.37 * L), 0.9),
        BRICK: ((0.44 * L, 0.14 * L, 0.08 * L), 0.9),
        TIMBER: ((0.07, 0.048, 0.033), 0.8),
        FRAME: ((0.12, 0.075, 0.042), 0.75),
        DOOR: ((0.17, 0.105, 0.055), 0.7),
        GLASS: ((0.20, 0.16, 0.10), 0.2),
    },
    # masks hooks per slot: (snow capacity, porosity, night emissive).
    "hooks": {WALLS: (1.0, 0.5, 0.0), ROOF: (1.0, 0.35, 0.0), STONE: (1.0, 0.4, 0.0), BRICK: (1.0, 0.45, 0.0),
              TIMBER: (1.0, 0.6, 0.0), FRAME: (1.0, 0.5, 0.0), DOOR: (1.0, 0.5, 0.0), GLASS: (0.0, 0.05, 1.0)},
    "slots": {"walls": WALLS, "roof": ROOF, "stone": STONE, "brick": BRICK, "timber": TIMBER, "frame": FRAME,
              "door": DOOR, "glass": GLASS, "sill": STONE, "pots": ROOF},
    # (u, width, height) on the front wall, whose u runs along +y.
    "door": (-0.05, 0.9, 1.84),
    # Casements: (wall, u, sill z, width, height, major). Walls: F front (u = y), HB hall back (u = -y),
    # WB wing back (u = -y), WR wing side (u = -x), WL wing's inner side (u = x), HL hall gable (u = x).
    # Major windows keep their frame at LOD1 and their sill and bars at LOD0.
    "windows": [("F", -2.1, 0.85, 1.0, 0.9, True), ("F", 1.9, 0.85, 1.1, 0.9, True),
                ("F", 1.9, 2.55, 0.5, 0.55, False),
                ("WR", 0.0, 0.85, 0.9, 0.85, False), ("WB", -1.9, 0.9, 0.8, 0.8, False),
                ("WL", -1.5, 0.95, 0.6, 0.75, False),
                ("HB", 0.7, 0.9, 0.8, 0.8, False), ("HB", 2.3, 0.9, 0.8, 0.8, False),
                ("HL", 0.7, 0.9, 0.8, 0.8, False)],
    # Framing: (wall, u0, z0, u1, z1, kept at LOD1, width). Rails run 5 cm past a corner so the members
    # on the two walls close it, and stop at the door.
    "timbers": [
        # Front, hall: corner post, post between window and door, junction post, sole and plate.
        ("F", -3.12, SOLE, -3.12, HPLATE + 0.07, True, 0.16), ("F", -1.15, SOLE, -1.15, HPLATE, True, 0.15),
        ("F", -3.25, SOLE, -0.6, SOLE, True, 0.15), ("F", -3.25, HPLATE, 0.68, HPLATE, True, 0.15),
        ("F", -2.1, SOLE, -2.1, 0.68, False, 0.12), ("F", -3.04, 1.3, -2.8, 1.94, False, 0.11),
        # Front, wing: close-studded gable wall with a tie beam.
        ("F", 0.68, SOLE, 0.68, TIE + 0.08, True, 0.16), ("F", 3.12, SOLE, 3.12, TIE + 0.08, True, 0.16),
        ("F", 0.6, SOLE, 3.25, SOLE, True, 0.15), ("F", 0.6, TIE, 3.25, TIE, True, 0.16),
        ("F", 1.02, SOLE, 1.02, TIE, False, 0.12), ("F", 2.78, SOLE, 2.78, TIE, False, 0.12),
        ("F", 1.6, 1.84, 1.6, TIE, False, 0.1), ("F", 2.2, 1.84, 2.2, TIE, False, 0.1),
        ("F", 1.9, 3.19, 1.9, 3.85, False, 0.12),
        ("F", 1.32, TIE + 0.08, 1.32, 3.05, False, 0.12), ("F", 2.48, TIE + 0.08, 2.48, 3.05, False, 0.12),
        # Wing side (+y).
        ("WR", -2.25, SOLE, 2.25, SOLE, True, 0.15), ("WR", -2.25, WPLATE, 2.25, WPLATE, True, 0.15),
        ("WR", -2.12, SOLE, -2.12, WPLATE + 0.07, True, 0.16), ("WR", 2.12, SOLE, 2.12, WPLATE + 0.07, True, 0.16),
        ("WR", -0.8, SOLE, -0.8, WPLATE, True, 0.15), ("WR", 0.8, SOLE, 0.8, WPLATE, True, 0.15),
        ("WR", -2.0, 0.5, -0.9, 1.86, False, 0.12), ("WR", 2.0, 0.5, 0.9, 1.86, False, 0.12),
        # Wing back gable.
        ("WB", -3.25, SOLE, -0.55, SOLE, True, 0.15), ("WB", -3.25, TIE, -0.55, TIE, True, 0.16),
        ("WB", -3.12, SOLE, -3.12, TIE + 0.08, True, 0.16), ("WB", -0.68, SOLE, -0.68, TIE + 0.08, True, 0.16),
        ("WB", -2.6, SOLE, -2.6, TIE, False, 0.12), ("WB", -1.2, SOLE, -1.2, TIE, False, 0.12),
        ("WB", -1.9, TIE + 0.08, -1.9, 3.85, False, 0.12),
        # Wing's inner side, behind the hall.
        ("WL", -2.25, SOLE, -0.8, SOLE, True, 0.15), ("WL", -2.25, WPLATE, -0.8, WPLATE, True, 0.15),
        ("WL", -2.12, SOLE, -2.12, WPLATE + 0.07, True, 0.16), ("WL", -0.88, SOLE, -0.88, WPLATE + 0.07, True, 0.16),
        # Hall back.
        ("HB", -0.6, SOLE, 3.25, SOLE, True, 0.15), ("HB", -0.6, HPLATE, 3.25, HPLATE, True, 0.15),
        ("HB", 3.12, SOLE, 3.12, HPLATE + 0.07, True, 0.16), ("HB", -0.52, SOLE, -0.52, HPLATE + 0.07, True, 0.16),
        ("HB", 1.5, SOLE, 1.5, HPLATE, True, 0.15),
        ("HB", 0.7, SOLE, 0.7, 0.81, False, 0.1), ("HB", 2.3, SOLE, 2.3, 0.81, False, 0.1),
        # Hall gable.
        ("HL", -0.85, SOLE, 2.25, SOLE, True, 0.15), ("HL", -0.85, TIE, 2.25, TIE, True, 0.16),
        ("HL", -0.72, SOLE, -0.72, TIE + 0.08, True, 0.16), ("HL", 2.12, SOLE, 2.12, TIE + 0.08, True, 0.16),
        ("HL", -0.05, SOLE, -0.05, TIE, False, 0.12), ("HL", 1.45, SOLE, 1.45, TIE, False, 0.12),
        ("HL", 0.7, TIE + 0.08, 0.7, 3.2, False, 0.12),
    ],
}

_house_longhouse.build_piece(SPEC, __file__)
