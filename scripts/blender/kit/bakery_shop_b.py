"""bakery_shop_b: the twin-gabled bakehouse, a hero variant of bakery_shop for the bakery tile's shop slot
(PRD §6 Variety; owner-approved refs assets-source/refs/bakery ref-1 and ref-2, A17): a timber-framed
village bakery with a hipped clay-tile roof on low eaves, two front gables with casements (ref-1), a
broad brick bread-oven stack astride the ridge, a flat shelved shop window of loaves under an oak fascia
beam, one long clay-tile pentice over the window and the half-glazed door, and bakery_shop's bench and
bread basket. Scripted route; the builder, the slot fit and the pipeline are in _bakery_b.py.

Slots and colours are bakery_shop's (same palette, same hooks), so both pieces read as one bakery family:
isotropic surfaces sample the world-space `plaster` set tinted per slot, directional detail (tile
courses, framing, joinery, glazing bars) is geometry, and the one unique map is a 1024² masks atlas.
The shop front faces +x (yaw 0)."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import _bakery_b  # noqa: E402
from _house import PLASTER_LIN as L  # noqa: E402

P = "bakery_shop_b"
NAMES = ("walls", "roof", "brick", "stone", "timber", "frame", "door", "glass", "display_glass", "loaf_glass",
         "wicker", "bread")
S = {n: f"{P}_{n}" for n in NAMES}
# Manifest colour and roughness per slot (bakery_shop's). Tileable slots multiply the plaster set's
# albedo at runtime, so their Blender colour is the manifest colour x PLASTER_LIN.
TILEABLE = ("walls", "roof", "brick", "stone")
COLOURS = {
    "walls": ((1.0, 0.9, 0.66), 0.9),
    "roof": ((0.24, 0.12, 0.08), 0.85),
    "brick": ((0.44, 0.14, 0.08), 0.9),
    "stone": ((0.45, 0.42, 0.37), 0.9),
    "timber": ((0.07, 0.048, 0.033), 0.8),
    "frame": ((0.12, 0.075, 0.042), 0.75),
    "door": ((0.17, 0.105, 0.055), 0.7),
    "glass": ((0.2, 0.16, 0.1), 0.2),
    "display_glass": ((0.16, 0.1, 0.055), 0.25),
    "loaf_glass": ((0.55, 0.29, 0.1), 0.3),
    "wicker": ((0.45, 0.3, 0.14), 0.85),
    "bread": ((0.55, 0.27, 0.085), 0.6),
}
# masks hooks per slot: (snow capacity, porosity, night emissive), bakery_shop's.
HOOKS = {"walls": (1.0, 0.5, 0.0), "roof": (1.0, 0.35, 0.0), "brick": (1.0, 0.45, 0.0),
         "stone": (1.0, 0.4, 0.0), "timber": (1.0, 0.6, 0.0), "frame": (1.0, 0.5, 0.0),
         "door": (1.0, 0.5, 0.0), "glass": (0.0, 0.05, 1.0), "display_glass": (0.0, 0.05, 1.0),
         "loaf_glass": (0.0, 0.05, 1.0), "wicker": (1.0, 0.6, 0.0), "bread": (1.0, 0.4, 0.0)}

SPEC = {
    "piece": P,
    "materials": {S[n]: (tuple(c * (L if n in TILEABLE else 1.0) for c in col), rough)
                  for n, (col, rough) in COLOURS.items()},
    "hooks": {S[n]: HOOKS[n] for n in NAMES},
    "slots": {**S, "sill": S["frame"]},
}

_bakery_b.build_piece(SPEC, __file__)
