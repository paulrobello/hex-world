"""house_cottage_b: a single-storey rendered cottage with a slate roof, the second house_cottage variant
for house_cluster's cottage slots (PRD §6 Variety; owner-approved refs, assets-source/refs/
house_cluster ref-1, its slate-roofed right-hand house): cream lime render with dressed stone quoins,
a grey slate roof at 40° on low eaves, a stone stack with terracotta pots rising through the ridge at
each gable end, a small gabled slate porch on oak posts over a plank door, oak bargeboards and oak
casements. Scripted route; the builder and its slot contract are in _house_slate.py (on _house.py's
and _house_longhouse.py's parts).

Plan (Blender x deep, y wide): walls 4.4 x 6.4 m (x ±2.2, y ±3.2), house_cottage's wall box; eaves
2.4 m (the cottage's 2.6), ridge 4.25 m; the porch 1.2 m wide and 0.75 m deep on the door at y 0, its
posts and bargeboards inside the parking test's doorstep box. The door faces +x (yaw 0)."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _house_slate  # noqa: E402
from _house import PLASTER_LIN as L  # noqa: E402

P = "house_cottage_b"
WALLS, ROOF, STONE, TIMBER, DOOR, GLASS, POTS = (
    f"{P}_{n}" for n in ("walls", "roof", "stone", "timber", "door", "glass", "pots"))
SPEC = {
    "piece": P,
    "depth": 4.4, "width": 6.4, "eave": 2.4, "pitch": 40.0,
    "overhang": 0.3, "gable_overhang": 0.18, "courses": 14, "kick": 0.025, "roof_thick": 0.14,
    # Stacks through the ridge, inside each gable wall: (centre y) per stack, (x, y) size, height above
    # the ridge (_house_longhouse.chimney).
    "stacks": (-2.65, 2.65), "chimney_size": (0.6, 0.9), "chimney_above": 0.75,
    # Porch: (u, width, depth out from the wall to the posts' face, eave height, pitch, overhang, slab).
    "porch": (0.0, 1.2, 0.75, 2.40, 45.0, 0.06, 0.1), "porch_courses": 3, "porch_kick": 0.02,
    "porch_front_over": 0.1,
    # Quoins: (count per corner, block height, vertical step).
    "quoins": (5, 0.3, 0.4),
    # Blender colours (linear). Tileable slots show manifest colour x plaster albedo (PLASTER_LIN).
    "materials": {
        WALLS: ((0.98 * L, 0.94 * L, 0.84 * L), 0.9),
        ROOF: ((0.105 * L, 0.11 * L, 0.125 * L), 0.7),
        STONE: ((0.48 * L, 0.45 * L, 0.40 * L), 0.9),
        TIMBER: ((0.16, 0.10, 0.055), 0.75),
        DOOR: ((0.20, 0.12, 0.06), 0.7),
        GLASS: ((0.20, 0.16, 0.10), 0.2),
        POTS: ((0.50 * L, 0.19 * L, 0.095 * L), 0.85),
    },
    # masks hooks per slot: (snow capacity, porosity, night emissive).
    "hooks": {WALLS: (1.0, 0.5, 0.0), ROOF: (1.0, 0.2, 0.0), STONE: (1.0, 0.4, 0.0), TIMBER: (1.0, 0.5, 0.0),
              DOOR: (1.0, 0.5, 0.0), GLASS: (0.0, 0.05, 1.0), POTS: (1.0, 0.35, 0.0)},
    "slots": {"walls": WALLS, "roof": ROOF, "stone": STONE, "brick": STONE, "timber": TIMBER, "frame": TIMBER,
              "door": DOOR, "glass": GLASS, "sill": STONE, "pots": POTS},
    # (u, width, height) on the front wall, whose u runs along +y.
    "door": (0.0, 0.9, 1.95),
    # Casements: (wall, u, sill z, width, height, major). Wall u runs along +y on the front, -y on the
    # back, +x on the left gable (-y) and -x on the right gable (+y). Major windows keep their frame at
    # LOD1 and their sill and bars at LOD0.
    "windows": [("front", -1.75, 0.85, 1.1, 1.0, True), ("front", 1.75, 0.85, 1.1, 1.0, True),
                ("back", -1.5, 0.9, 0.9, 0.9, False), ("back", 1.5, 0.9, 0.9, 0.9, False),
                ("left", 0.0, 2.7, 0.55, 0.6, False), ("right", 0.6, 0.9, 0.8, 0.9, False)],
}

_house_slate.build_piece(SPEC, __file__)
