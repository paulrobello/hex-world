"""well_stone: the covered village well (owner-approved ref-1, assets-source/refs/well_stone, 2026-10-20):
a round waist-high fieldstone wall of rough grey and buff stones in pale lime mortar under a ring of
pale dressed capstones, on a round flagstone apron; two stout dark oak posts carry a small steep
gabled roof of clay tiles (house_cottage's roof colour) with dark oak bargeboards, a turned windlass
with an iron crank, and hemp rope coiled on the drum and hanging in a slack loop to a stave bucket
with iron hoops resting on the capstones. Scripted route; the builder and its slot contract are in
_house_well.py (on _house.py's and _house_longhouse.py's parts).

Size (owner WQ3 recommendation, not yet answered): 1.58 m across the capstones, the wall 0.9 m high,
the ridge 2.4 m up, the apron 2.7 m across. Placement (WQ1 recommendation): one well on house_cluster's
lawn beside cottage 0 (data/tiles/house_cluster.ron hero 3). The open front (the bucket) faces +x."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _house_well  # noqa: E402
from _house import PLASTER_LIN as L  # noqa: E402

P = "well_stone"
STONE, BUFF, MORTAR, CAP, ROOF, TIMBER, WOOD, ROPE, IRON = (
    f"{P}_{n}" for n in ("stone", "stone_buff", "mortar", "cap", "roof", "timber", "wood", "rope", "iron"))
SPEC = {
    "piece": P,
    "roof": True,
    "seed": "well_stone",  # well_stone_b draws the same wall and apron
    "windlass_z": 1.10,
    # The bucket on the capstones at the front right, the rope from the coil's front sagging over the
    # opening to its bail.
    "bucket": (0.60, 0.24, _house_well.CAP_Z1),
    "rope_y": 0.02,
    "rope": [(0.16, 0.07, 0.99), (0.36, 0.16, 1.0)],
    # Blender colours (linear). Tileable slots show manifest colour x plaster albedo (PLASTER_LIN).
    "materials": {
        STONE: ((0.47 * L, 0.46 * L, 0.43 * L), 0.9),
        BUFF: ((0.60 * L, 0.50 * L, 0.37 * L), 0.9),
        MORTAR: ((0.84 * L, 0.79 * L, 0.67 * L), 0.9),
        CAP: ((0.72 * L, 0.70 * L, 0.66 * L), 0.85),
        ROOF: ((0.50 * L, 0.19 * L, 0.095 * L), 0.85),
        TIMBER: ((0.12, 0.075, 0.045), 0.8),
        WOOD: ((0.24, 0.15, 0.08), 0.7),
        ROPE: ((0.42, 0.33, 0.20), 0.9),
        IRON: ((0.05, 0.05, 0.055), 0.55),
    },
    # masks hooks per slot: (snow capacity, porosity, night emissive).
    "hooks": {STONE: (1.0, 0.4, 0.0), BUFF: (1.0, 0.4, 0.0), MORTAR: (1.0, 0.5, 0.0), CAP: (1.0, 0.35, 0.0),
              ROOF: (1.0, 0.35, 0.0), TIMBER: (1.0, 0.6, 0.0), WOOD: (1.0, 0.55, 0.0), ROPE: (0.8, 0.7, 0.0),
              IRON: (0.6, 0.15, 0.0)},
    "slots": {"stone": STONE, "stone_buff": BUFF, "mortar": MORTAR, "cap": CAP, "roof": ROOF, "timber": TIMBER,
              "wood": WOOD, "rope": ROPE, "iron": IRON},
}

_house_well.build_piece(SPEC, __file__)
