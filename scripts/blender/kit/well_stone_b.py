"""well_stone_b: the open village well, well_stone's variant for house_cluster's well slot (PRD §6
Variety; owner-approved ref-2, assets-source/refs/well_stone, 2026-10-20): well_stone's fieldstone wall,
capstones and flagstone apron (the same seed), its two oak posts taller and joined by a crossbeam, the
turned windlass with its iron crank and hemp rope, and the stave bucket hanging just above the opening.
No roof. Scripted route; the builder and its slot contract are in _house_well.py.

Posts 2.16 m, the crossbeam 1.92-2.04 m, the windlass at 1.55 m, the bucket's foot at 0.98 m (8 cm over
the capstones). The open front faces +x, as well_stone's."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _house_well  # noqa: E402
from _house import PLASTER_LIN as L  # noqa: E402

P = "well_stone_b"
STONE, BUFF, MORTAR, CAP, TIMBER, WOOD, ROPE, IRON = (
    f"{P}_{n}" for n in ("stone", "stone_buff", "mortar", "cap", "timber", "wood", "rope", "iron"))
SPEC = {
    "piece": P,
    "roof": False,
    "seed": "well_stone",
    "post_top": 2.16,
    "crossbeam": (1.92, 2.04),
    "windlass_z": 1.55,
    # The bucket hangs straight down from the coil's front.
    "bucket": (_house_well.COIL_R, 0.0, 0.98),
    "rope_y": 0.0,
    "rope": [],
    # Blender colours (linear), well_stone's. Tileable slots show manifest colour x plaster albedo.
    "materials": {
        STONE: ((0.47 * L, 0.46 * L, 0.43 * L), 0.9),
        BUFF: ((0.60 * L, 0.50 * L, 0.37 * L), 0.9),
        MORTAR: ((0.84 * L, 0.79 * L, 0.67 * L), 0.9),
        CAP: ((0.72 * L, 0.70 * L, 0.66 * L), 0.85),
        TIMBER: ((0.12, 0.075, 0.045), 0.8),
        WOOD: ((0.24, 0.15, 0.08), 0.7),
        ROPE: ((0.42, 0.33, 0.20), 0.9),
        IRON: ((0.05, 0.05, 0.055), 0.55),
    },
    # masks hooks per slot: (snow capacity, porosity, night emissive).
    "hooks": {STONE: (1.0, 0.4, 0.0), BUFF: (1.0, 0.4, 0.0), MORTAR: (1.0, 0.5, 0.0), CAP: (1.0, 0.35, 0.0),
              TIMBER: (1.0, 0.6, 0.0), WOOD: (1.0, 0.55, 0.0), ROPE: (0.8, 0.7, 0.0), IRON: (0.6, 0.15, 0.0)},
    "slots": {"stone": STONE, "stone_buff": BUFF, "mortar": MORTAR, "cap": CAP, "timber": TIMBER, "wood": WOOD,
              "rope": ROPE, "iron": IRON},
}

_house_well.build_piece(SPEC, __file__)
