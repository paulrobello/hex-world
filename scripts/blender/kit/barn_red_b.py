"""barn_red_b: the farm barn's catslide variant (PRD §6 Variety; P3 farm, owner-approved refs
assets-source/refs/farm, A17: pitched roof, red board barn, dark slate): a lower red board-and-batten
barn whose slate roof runs on past its -y wall as a 22° catslide over an open lean-to shed on white
posts (ref-1's lean-to), with a louvred cupola on the ridge and cross-braced double doors and a
hayloft door on the +x gable, which faces the hay as barn_red's does. Scripted route; the builder,
its plan and its slot fit are in _barn_b.py, on the house family's pipeline (_house_longhouse.py).

Plan (Blender x along the ridge, y across): main barn 10.0 x 4.5 m (x ±5.0, y -1.0..3.5), eave line
3.8 m, ridge 6.05 m, cupola cap 7.6 m; shed 2.45 m deep to the post line at y -3.45; roof to y ±3.85
and x ±5.3, inside barn_red's collider box. Budget: prop (LOD0 <= 1,500 tris), KTX2 class `piece`,
one 1024² masks."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _barn_b  # noqa: E402
from _house import PLASTER_LIN as L  # noqa: E402

P = "barn_red_b"
WALLS, DOOR, ROOF, STONE, TRIM, GLASS = (f"{P}_{n}" for n in ("walls", "door", "roof", "stone", "trim", "glass"))
SPEC = {
    "piece": P,
    # Blender colours (linear): barn_red's slots. Tileable slots show manifest colour x plaster albedo
    # (PLASTER_LIN), so the stills match the runtime; the manifest carries the bare colours.
    "materials": {
        WALLS: ((0.45 * L, 0.07 * L, 0.045 * L), 0.9),
        DOOR: ((0.32 * L, 0.055 * L, 0.04 * L), 0.85),
        ROOF: ((0.105 * L, 0.11 * L, 0.125 * L), 0.7),
        STONE: ((0.42 * L, 0.39 * L, 0.34 * L), 0.9),
        TRIM: ((0.70, 0.69, 0.65), 0.6),
        GLASS: ((0.20, 0.16, 0.10), 0.2),
    },
    # masks hooks per slot: (snow capacity, porosity, night emissive), barn_red's.
    "hooks": {WALLS: (1.0, 0.55, 0.0), DOOR: (1.0, 0.55, 0.0), ROOF: (1.0, 0.2, 0.0), STONE: (1.0, 0.4, 0.0),
              TRIM: (1.0, 0.4, 0.0), GLASS: (0.0, 0.05, 1.0)},
    "slots": {"walls": WALLS, "door": DOOR, "roof": ROOF, "stone": STONE, "trim": TRIM, "glass": GLASS},
}

_barn_b.build_piece(SPEC, __file__)
